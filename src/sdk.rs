use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::Duration;

use superhuman_docs_async::{Client, ClientOptions, Error, Response, DEFAULT_BASE_URL};

use crate::model::SuperhumanDocsClientConfig;

mod transport;

use transport::{HttpTransport, TransportState};

pub(crate) fn normalize_api_base(base: &str) -> String {
    base.trim_end_matches('/').to_string()
}

pub(crate) fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

pub(crate) struct SdkClient {
    client: Client,
    state: Arc<Mutex<TransportState>>,
    execution: tokio::sync::Mutex<()>,
}

pub(crate) type OperationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'a>>;

impl SdkClient {
    pub(crate) fn new(config: &SuperhumanDocsClientConfig) -> Result<Self, String> {
        let base_url = if config.endpoint.is_empty() {
            DEFAULT_BASE_URL
        } else {
            config.endpoint.as_str()
        };
        Self::at(base_url, &config.credential)
    }

    pub(crate) fn at(base_url: &str, _credential: &str) -> Result<Self, String> {
        let state = Arc::new(Mutex::new(TransportState::default()));
        let transport = HttpTransport::new(Arc::clone(&state), _credential.to_string())
            .map_err(|error| error.to_string())?;
        Self::with_transport(base_url, state, transport)
    }

    #[cfg(test)]
    pub(crate) fn at_with_timeout(
        base_url: &str,
        credential: &str,
        timeout: Duration,
    ) -> Result<Self, String> {
        let state = Arc::new(Mutex::new(TransportState::default()));
        let transport =
            HttpTransport::new_with_timeout(Arc::clone(&state), credential.to_string(), timeout)
                .map_err(|error| error.to_string())?;
        Self::with_transport(base_url, state, transport)
    }

    fn with_transport(
        base_url: &str,
        state: Arc<Mutex<TransportState>>,
        transport: HttpTransport,
    ) -> Result<Self, String> {
        let options = ClientOptions::new(transport).with_base_url(normalize_api_base(base_url));
        let client = Client::new(options).map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            state,
            execution: tokio::sync::Mutex::new(()),
        })
    }

    pub(crate) async fn execute<T, F>(&self, operation: F) -> Result<String, String>
    where
        F: for<'a> FnOnce(&'a Client) -> OperationFuture<'a, T>,
    {
        self.execute_inner(None, operation)
            .await?
            .ok_or_else(|| "SDK transport returned an unexpected accepted status".to_string())
    }

    pub(crate) async fn execute_accepting_status<T, F>(
        &self,
        accepted_status: u16,
        operation: F,
    ) -> Result<Option<String>, String>
    where
        F: for<'a> FnOnce(&'a Client) -> OperationFuture<'a, T>,
    {
        self.execute_inner(Some(accepted_status), operation).await
    }

    async fn execute_inner<T, F>(
        &self,
        accepted_status: Option<u16>,
        operation: F,
    ) -> Result<Option<String>, String>
    where
        F: for<'a> FnOnce(&'a Client) -> OperationFuture<'a, T>,
    {
        let _execution = self.execution.lock().await;
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "HTTP transport state lock poisoned".to_string())?;
            state.exchange = None;
        }

        let result = operation(&self.client).await;
        let exchange = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "HTTP transport state lock poisoned".to_string())?;
            state.exchange.take()
        };

        match (result, exchange) {
            (Ok(_), Some(exchange)) => response_body(exchange.response).map(Some),
            (Err(Error::Deserialize { .. }), Some(exchange))
                if exchange.response.status == exchange.expected_status =>
            {
                response_body(exchange.response).map(Some)
            }
            (Err(Error::UnexpectedStatus { actual, .. }), Some(_))
                if accepted_status == Some(actual) =>
            {
                Ok(None)
            }
            (Err(error), _) => Err(error.to_string()),
            (Ok(_), None) => Err("SDK transport returned no response".to_string()),
        }
    }

    #[cfg(test)]
    pub(crate) async fn send_raw(
        &self,
        request: superhuman_docs_async::Request,
    ) -> Result<Response, Error> {
        self.client.send_request(request).await
    }
}

fn response_body(response: Response) -> Result<String, String> {
    String::from_utf8(response.body).map_err(|error| error.to_string())
}

pub(crate) async fn validate_token_at(base_url: &str, credential: &str) -> Result<(), String> {
    let sdk = SdkClient::at(base_url, credential)?;
    sdk.execute(|client| {
        Box::pin(async move {
            client
                .whoami(superhuman_docs_async::operations::WhoamiInput {})
                .await
        })
    })
    .await?;
    Ok(())
}

pub(crate) async fn validate_token(credential: &str) -> Result<(), String> {
    validate_token_at(DEFAULT_BASE_URL, credential).await
}
