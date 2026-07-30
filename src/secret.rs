use crate::attach::read_environment_variable;
use crate::constants::{SECRET_SCOPE_PREFIX, SECRET_TYPE};
use crate::ffi::*;
use crate::sdk::validate_token;

fn option_value(value: RustExtInputValue) -> Result<String, String> {
    match value.value_type {
        1 => Ok(value.bool_value.to_string()),
        2 => Ok(value.int_value.to_string()),
        3 => Ok(value.uint_value.to_string()),
        4 => Ok(value.double_value.to_string()),
        5 => Ok(value.string_value.as_str().to_string()),
        _ => Err("secret option must not be NULL".to_string()),
    }
}

pub(crate) async fn create_secret(
    input: RustExtSecretCreateInput,
) -> Result<RustExtSecretCreateResult, String> {
    let secret_type = input.secret_type.as_str();
    if secret_type != std::str::from_utf8(&SECRET_TYPE[..SECRET_TYPE.len() - 1]).unwrap_or("") {
        return Err(format!(
            "No Superhuman Docs secret configuration registered for type: {secret_type}"
        ));
    }

    let mut credential = None;
    for option in slice_from_raw_parts(input.options, input.option_count) {
        let name = option.name.as_str();
        let value = option_value(option.value)?;
        let resolved = if name.eq_ignore_ascii_case("token") {
            value
        } else if name.eq_ignore_ascii_case("token_env") {
            read_environment_variable(&value)?
        } else {
            return Err(format!(
                "Unknown named parameter for {secret_type} secret: {name}"
            ));
        };
        if credential.replace(resolved).is_some() {
            return Err("TOKEN and TOKEN_ENV cannot both be specified".to_string());
        }
    }

    let entry = if let Some(credential) = credential {
        validate_token(&credential).await?;
        Some(RustExtNamedValue {
            name: alloc_string("token"),
            value: RustExtInputValue {
                value_type: 5,
                string_value: alloc_string(&credential),
                ..Default::default()
            },
        })
    } else {
        None
    };
    let scope = if input.scope_count == 0 {
        vec![alloc_string(SECRET_SCOPE_PREFIX)]
    } else {
        slice_from_raw_parts(input.scope, input.scope_count)
            .iter()
            .map(|scope| alloc_string(scope.as_str()))
            .collect()
    };
    let entries = entry.into_iter().collect();
    let redact_keys = vec![alloc_string("token")];
    let (scope, scope_count) = vec_into_raw_parts(scope);
    let (entries, entry_count) = vec_into_raw_parts(entries);
    let (redact_keys, redact_key_count) = vec_into_raw_parts(redact_keys);
    Ok(RustExtSecretCreateResult {
        scope,
        scope_count,
        entries,
        entry_count,
        redact_keys,
        redact_key_count,
    })
}

pub(crate) fn free_secret(secret: RustExtSecretCreateResult) {
    for scope in vec_from_raw_parts(secret.scope, secret.scope_count) {
        scope.free();
    }
    for entry in vec_from_raw_parts(secret.entries, secret.entry_count) {
        entry.name.free();
        if entry.value.value_type == 5 {
            entry.value.string_value.free();
        }
    }
    for key in vec_from_raw_parts(secret.redact_keys, secret.redact_key_count) {
        key.free();
    }
}
