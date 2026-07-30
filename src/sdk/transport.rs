use std::sync::{Arc, Mutex};
use std::time::Duration;

use superhuman_docs_async::{Error, Request, Response, Transport, TransportFuture};

#[cfg(target_os = "emscripten")]
mod fetch;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct Exchange {
    pub(super) expected_status: u16,
    pub(super) response: Response,
}

#[derive(Default)]
pub(super) struct TransportState {
    pub(super) exchange: Option<Exchange>,
}

pub(super) struct HttpTransport {
    pub(super) state: Arc<Mutex<TransportState>>,
    pub(super) credential: String,
    #[cfg(target_os = "emscripten")]
    timeout: Duration,
    #[cfg(not(target_os = "emscripten"))]
    client: reqwest::Client,
}

impl HttpTransport {
    pub(super) fn new(
        state: Arc<Mutex<TransportState>>,
        credential: String,
    ) -> Result<Self, Error> {
        Self::new_with_timeout(state, credential, REQUEST_TIMEOUT)
    }

    pub(super) fn new_with_timeout(
        state: Arc<Mutex<TransportState>>,
        credential: String,
        timeout: Duration,
    ) -> Result<Self, Error> {
        #[cfg(not(target_os = "emscripten"))]
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(Error::transport)?;

        Ok(Self {
            state,
            credential,
            #[cfg(target_os = "emscripten")]
            timeout,
            #[cfg(not(target_os = "emscripten"))]
            client,
        })
    }

    async fn dispatch(&self, request: Request) -> Result<Response, Error> {
        let expected_status = request.expected_status;
        let response = send_http_request(self, request).await?;
        self.state
            .lock()
            .map_err(|_| Error::transport("HTTP transport state lock poisoned"))?
            .exchange = Some(Exchange {
            expected_status,
            response: response.clone(),
        });
        Ok(response)
    }
}

impl Transport for HttpTransport {
    fn send_request(&self, request: Request) -> TransportFuture<'_> {
        Box::pin(self.dispatch(request))
    }
}

#[cfg(not(target_os = "emscripten"))]
async fn send_http_request(transport: &HttpTransport, request: Request) -> Result<Response, Error> {
    let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(Error::transport)?;
    let mut builder = transport
        .client
        .request(method, &request.url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", transport.credential),
        )
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    let response = builder.send().await.map_err(Error::transport)?;
    let status = response.status().as_u16();
    let body = response.bytes().await.map_err(Error::transport)?.to_vec();
    Ok(Response { status, body })
}

#[cfg(target_os = "emscripten")]
async fn send_http_request(transport: &HttpTransport, request: Request) -> Result<Response, Error> {
    fetch::send(request, &transport.credential, transport.timeout).await
}
