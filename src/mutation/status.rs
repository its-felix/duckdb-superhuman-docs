use serde_json::Value;
use std::future::Future;
use std::time::{Duration, Instant};
use superhuman_docs_async::operations;

use crate::model::SuperhumanDocsClientConfig;
use crate::sdk::SdkClient;

const MUTATION_POLL_INTERVAL: Duration = Duration::from_secs(1);

fn mutation_request_id(body: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid mutation response: {error}"))?;
    root.get("requestId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "Superhuman Docs mutation response did not contain requestId".to_string())
}

fn mutation_status(body: &str) -> Result<(bool, Option<String>), String> {
    let root: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid mutation status response: {error}"))?;
    let completed = root
        .get("completed")
        .and_then(Value::as_bool)
        .ok_or("mutation status response did not contain completed")?;
    let warning = match root.get("warning") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.trim().is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err("mutation status warning was not a string".to_string()),
    };
    Ok((completed, warning))
}

async fn poll_mutation_status<Fetch, FetchFuture, Elapsed, Sleep, SleepFuture>(
    request_id: &str,
    timeout: Duration,
    allow_warnings: bool,
    mut fetch: Fetch,
    mut elapsed: Elapsed,
    mut sleep: Sleep,
) -> Result<(), String>
where
    Fetch: FnMut() -> FetchFuture,
    FetchFuture: Future<Output = Result<String, String>>,
    Elapsed: FnMut() -> Duration,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    let mut first_check = true;
    loop {
        if !first_check && elapsed() >= timeout {
            return Err(format!(
                "Superhuman Docs mutation {request_id} did not complete within {} seconds; remote completion is unknown and may occur later",
                timeout.as_secs()
            ));
        }
        first_check = false;

        let body = fetch().await.map_err(|error| {
            format!("failed to check Superhuman Docs mutation {request_id}: {error}")
        })?;
        let (completed, warning) = mutation_status(&body).map_err(|error| {
            format!("failed to check Superhuman Docs mutation {request_id}: {error}")
        })?;
        if completed {
            if let Some(warning) = warning {
                if !allow_warnings {
                    return Err(format!(
                        "Superhuman Docs mutation {request_id} completed with a warning and cannot be rolled back: {warning}"
                    ));
                }
            }
            return Ok(());
        }

        let elapsed_now = elapsed();
        if elapsed_now >= timeout {
            return Err(format!(
                "Superhuman Docs mutation {request_id} did not complete within {} seconds; remote completion is unknown and may occur later",
                timeout.as_secs()
            ));
        }
        sleep(MUTATION_POLL_INTERVAL.min(timeout - elapsed_now)).await;
    }
}

pub(super) async fn wait_for_mutation(
    sdk: &SdkClient,
    config: &SuperhumanDocsClientConfig,
    response_body: &str,
) -> Result<(), String> {
    if !config.wait_for_mutations {
        return Ok(());
    }
    let request_id = mutation_request_id(response_body)?;
    let timeout = Duration::from_secs(config.mutation_timeout_seconds);
    let started = Instant::now();
    poll_mutation_status(
        &request_id,
        timeout,
        config.allow_mutation_warnings,
        || {
            let request_id = request_id.clone();
            async move {
                let response = sdk
                    .execute_accepting_status(404, |client| {
                        Box::pin(async move {
                            client
                                .mutation_status()
                                .read(operations::GetMutationStatusInput { request_id })
                                .await
                        })
                    })
                    .await?;
                Ok(response.unwrap_or_else(|| r#"{"completed":false}"#.to_string()))
            }
        },
        || started.elapsed(),
        crate::platform::sleep,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::future::ready;

    use super::*;

    #[test]
    fn mutation_response_requires_request_id() {
        assert_eq!(
            mutation_request_id(r#"{"requestId":"request-1"}"#).unwrap(),
            "request-1"
        );
        assert!(mutation_request_id("{}").unwrap_err().contains("requestId"));
        assert!(mutation_request_id("not-json")
            .unwrap_err()
            .contains("invalid mutation response"));
    }

    #[test]
    fn mutation_status_requires_boolean_completion_and_string_warning() {
        assert_eq!(
            mutation_status(r#"{"completed":true}"#).unwrap(),
            (true, None)
        );
        assert_eq!(
            mutation_status(r#"{"completed":true,"warning":"caveat"}"#).unwrap(),
            (true, Some("caveat".to_string()))
        );
        assert!(mutation_status("{}").unwrap_err().contains("completed"));
        assert!(mutation_status(r#"{"completed":true,"warning":1}"#)
            .unwrap_err()
            .contains("not a string"));
    }

    #[test]
    fn polling_checks_immediately_and_retries_until_complete() {
        let responses = Cell::new(0_u32);
        let elapsed = Cell::new(Duration::ZERO);
        crate::platform::block_on_result(poll_mutation_status(
            "request-1",
            Duration::from_secs(5),
            false,
            || {
                let count = responses.get() + 1;
                responses.set(count);
                ready(Ok(if count == 1 {
                    r#"{"completed":false}"#.to_string()
                } else {
                    r#"{"completed":true}"#.to_string()
                }))
            },
            || elapsed.get(),
            |duration| {
                elapsed.set(elapsed.get() + duration);
                ready(())
            },
        ))
        .unwrap();
        assert_eq!(responses.get(), 2);
        assert_eq!(elapsed.get(), Duration::from_secs(1));
    }

    #[test]
    fn polling_applies_warning_policy() {
        let error = crate::platform::block_on_result(poll_mutation_status(
            "request-warning",
            Duration::from_secs(5),
            false,
            || {
                ready(Ok(
                    r#"{"completed":true,"warning":"partial result"}"#.to_string()
                ))
            },
            || Duration::ZERO,
            |_| ready(()),
        ))
        .unwrap_err();
        assert!(error.contains("completed with a warning"));
        assert!(error.contains("cannot be rolled back"));

        crate::platform::block_on_result(poll_mutation_status(
            "request-warning",
            Duration::from_secs(5),
            true,
            || {
                ready(Ok(
                    r#"{"completed":true,"warning":"partial result"}"#.to_string()
                ))
            },
            || Duration::ZERO,
            |_| ready(()),
        ))
        .unwrap();
    }

    #[test]
    fn polling_timeout_reports_ambiguous_remote_state() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut responses = VecDeque::from([
            r#"{"completed":false}"#.to_string(),
            r#"{"completed":false}"#.to_string(),
        ]);
        let error = crate::platform::block_on_result(poll_mutation_status(
            "request-timeout",
            Duration::from_secs(1),
            false,
            || ready(Ok(responses.pop_front().unwrap())),
            || elapsed.get(),
            |duration| {
                elapsed.set(elapsed.get() + duration);
                ready(())
            },
        ))
        .unwrap_err();
        assert!(error.contains("did not complete within 1 seconds"));
        assert!(error.contains("may occur later"));
    }
}
