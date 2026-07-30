use std::ffi::c_void;

use superhuman_docs_async::operations;
use superhuman_docs_async::DEFAULT_BASE_URL;

use crate::constants::{
    ALLOW_MUTATION_WARNINGS_OPTION, API_BASE_OPTION, INCLUDE_ROW_METADATA_OPTION,
    MUTATION_TIMEOUT_SECONDS_OPTION, SECRET_SCOPE_PREFIX, TOKEN_ENV_OPTION, TOKEN_OPTION,
    WAIT_FOR_MUTATIONS_OPTION,
};
use crate::ffi::*;
use crate::model::SuperhumanDocsClientConfig;
use crate::sdk::{normalize_api_base, SdkClient};

mod host;
mod resource;

pub(crate) use host::read_environment_variable;
use host::{owned_option, owned_secret};
pub(crate) use resource::{
    doc_id_from_browser_url, doc_id_from_resolved_link, is_browser_url,
    strip_attach_resource_prefix,
};

const DEFAULT_MUTATION_TIMEOUT_SECONDS: u64 = 60;

fn parse_boolean_option(name: &str, value: &str, default: bool) -> Result<bool, String> {
    match value {
        "" => Ok(default),
        "true" | "TRUE" | "True" | "1" => Ok(true),
        "false" | "FALSE" | "False" | "0" => Ok(false),
        _ => Err(format!("invalid boolean value for {name}: {value}")),
    }
}

fn parse_mutation_timeout(value: &str) -> Result<u64, String> {
    if value.is_empty() {
        return Ok(DEFAULT_MUTATION_TIMEOUT_SECONDS);
    }
    let seconds = value
        .parse::<u64>()
        .map_err(|_| format!("MUTATION_TIMEOUT_SECONDS must be a positive integer: {value}"))?;
    if seconds == 0 {
        return Err("MUTATION_TIMEOUT_SECONDS must be a positive integer".to_string());
    }
    Ok(seconds)
}

pub(crate) async fn resolve_attach(
    path: RustExtString,
    host: *const RustExtAttachHost,
    userdata: *mut c_void,
) -> Result<RustExtAttachConfig, String> {
    RustExtAttachHost::from_ptr(host)?;
    let token_value = owned_option(host, userdata, TOKEN_OPTION.as_ptr().cast())?;
    let token_env_value = owned_option(host, userdata, TOKEN_ENV_OPTION.as_ptr().cast())?;
    let endpoint_value = owned_option(host, userdata, API_BASE_OPTION.as_ptr().cast())?;
    let include_row_metadata_value =
        owned_option(host, userdata, INCLUDE_ROW_METADATA_OPTION.as_ptr().cast())?;
    let wait_for_mutations_value =
        owned_option(host, userdata, WAIT_FOR_MUTATIONS_OPTION.as_ptr().cast())?;
    let mutation_timeout_seconds_value = owned_option(
        host,
        userdata,
        MUTATION_TIMEOUT_SECONDS_OPTION.as_ptr().cast(),
    )?;
    let allow_mutation_warnings_value = owned_option(
        host,
        userdata,
        ALLOW_MUTATION_WARNINGS_OPTION.as_ptr().cast(),
    )?;

    if !token_value.is_empty() && !token_env_value.is_empty() {
        return Err("TOKEN and TOKEN_ENV cannot both be specified".to_string());
    }
    let explicit_credential = if token_env_value.is_empty() {
        token_value
    } else {
        read_environment_variable(&token_env_value)?
    };
    let input_resource = strip_attach_resource_prefix(path.as_str());
    if input_resource.is_empty() {
        return Err("empty doc id".to_string());
    }
    let include_system_columns =
        parse_boolean_option("INCLUDE_ROW_METADATA", &include_row_metadata_value, false)?;
    let wait_for_mutations =
        parse_boolean_option("WAIT_FOR_MUTATIONS", &wait_for_mutations_value, false)?;
    if !wait_for_mutations && !mutation_timeout_seconds_value.is_empty() {
        return Err("MUTATION_TIMEOUT_SECONDS requires WAIT_FOR_MUTATIONS true".to_string());
    }
    if !wait_for_mutations && !allow_mutation_warnings_value.is_empty() {
        return Err("ALLOW_MUTATION_WARNINGS requires WAIT_FOR_MUTATIONS true".to_string());
    }
    let mutation_timeout_seconds = parse_mutation_timeout(&mutation_timeout_seconds_value)?;
    let allow_mutation_warnings = parse_boolean_option(
        "ALLOW_MUTATION_WARNINGS",
        &allow_mutation_warnings_value,
        false,
    )?;
    let endpoint = normalize_api_base(if endpoint_value.is_empty() {
        DEFAULT_BASE_URL
    } else {
        &endpoint_value
    });

    let (resource, credential) = if is_browser_url(input_resource) {
        resolve_browser_url(
            input_resource,
            &explicit_credential,
            &endpoint,
            host,
            userdata,
        )
        .await?
    } else {
        let credential = if explicit_credential.is_empty() {
            resolve_stored_credential(input_resource, host, userdata)?
        } else {
            explicit_credential
        };
        (input_resource.to_string(), credential)
    };
    let config = Box::new(SuperhumanDocsClientConfig {
        resource,
        credential,
        endpoint,
        include_system_columns,
        wait_for_mutations,
        mutation_timeout_seconds,
        allow_mutation_warnings,
    });
    let handle = Box::into_raw(config);
    Ok(RustExtAttachConfig {
        handle: handle.cast(),
        database_name: borrow_string(unsafe { &*handle }.resource.as_str()),
    })
}

async fn resolve_browser_url(
    browser_url: &str,
    explicit_credential: &str,
    endpoint: &str,
    host: *const RustExtAttachHost,
    userdata: *mut c_void,
) -> Result<(String, String), String> {
    let bootstrap_credential = if !explicit_credential.is_empty() {
        explicit_credential.to_string()
    } else {
        let inferred = doc_id_from_browser_url(browser_url);
        let scoped = match inferred {
            Some(doc_id) => owned_secret(
                host,
                userdata,
                borrow_string(&format!("{SECRET_SCOPE_PREFIX}{doc_id}")),
            )?,
            None => String::new(),
        };
        if !scoped.is_empty() {
            scoped
        } else {
            let general = owned_secret(host, userdata, borrow_string(SECRET_SCOPE_PREFIX))?;
            if general.is_empty() {
                return Err(
                    "browser URL attachment requires TOKEN, TOKEN_ENV, a general superhuman_docs secret, or a canonical URL with a matching doc-scoped secret"
                        .to_string(),
                );
            }
            general
        }
    };

    let sdk = SdkClient::at(endpoint, &bootstrap_credential)?;
    let browser_url = browser_url.to_string();
    let body = sdk
        .execute(|client| {
            Box::pin(async move {
                client
                    .resolve_browser_link(operations::ResolveBrowserLinkInput {
                        url: browser_url,
                        degrade_gracefully: Some(false),
                    })
                    .await
            })
        })
        .await?;
    let doc_id = doc_id_from_resolved_link(&body)?;
    let credential = if explicit_credential.is_empty() {
        let canonical = owned_secret(
            host,
            userdata,
            borrow_string(&format!("{SECRET_SCOPE_PREFIX}{doc_id}")),
        )?;
        if canonical.is_empty() {
            bootstrap_credential
        } else {
            canonical
        }
    } else {
        bootstrap_credential
    };
    Ok((doc_id, credential))
}

fn resolve_stored_credential(
    resource: &str,
    host: *const RustExtAttachHost,
    userdata: *mut c_void,
) -> Result<String, String> {
    let primary_secret_scope = format!("{SECRET_SCOPE_PREFIX}{resource}");
    let value = owned_secret(host, userdata, borrow_string(&primary_secret_scope))?;
    if !value.is_empty() {
        return Ok(value);
    }
    let value = owned_secret(host, userdata, borrow_string(SECRET_SCOPE_PREFIX))?;
    if value.is_empty() {
        Err("missing credential".to_string())
    } else {
        Ok(value)
    }
}
