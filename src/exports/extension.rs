use std::ffi::c_void;

use crate::attach::resolve_attach;
use crate::constants::*;
use crate::ffi::*;
use crate::platform::block_on_result;

fn host_callback_result(success: bool, error: RustExtError, fallback: &str) -> Result<(), String> {
    let message = error.message.as_str().to_string();
    error.message.free();
    if success {
        Ok(())
    } else if message.is_empty() {
        Err(fallback.to_string())
    } else {
        Err(message)
    }
}

#[no_mangle]
pub extern "C" fn rust_ext_extension_load(
    host: *const RustExtDuckDbHost,
    loader: *mut c_void,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to load Superhuman Docs extension", || {
        let host = RustExtDuckDbHost::from_ptr(host)?;
        let mut host_error = RustExtError::default();
        let success =
            host.set_description(loader, c_static(EXTENSION_DESCRIPTION), &mut host_error);
        host_callback_result(success, host_error, "failed to set extension description")?;
        let parameters = [
            RustExtSecretParameter {
                name: c_static_string(TOKEN_OPTION),
                logical_type: borrow_string("VARCHAR"),
            },
            RustExtSecretParameter {
                name: c_static_string(TOKEN_ENV_OPTION),
                logical_type: borrow_string("VARCHAR"),
            },
        ];
        let registration = RustExtSecretRegistration {
            secret_type: c_static_string(SECRET_TYPE),
            provider: c_static_string(SECRET_PROVIDER),
            extension: c_static_string(EXTENSION_NAME),
            parameters: parameters.as_ptr(),
            parameter_count: parameters.len(),
        };
        let mut host_error = RustExtError::default();
        let success = host.register_secret(loader, registration, &mut host_error);
        host_callback_result(success, host_error, "failed to register config secret")?;
        let mut host_error = RustExtError::default();
        let success =
            host.register_storage_extension(loader, c_static(EXTENSION_NAME), &mut host_error);
        host_callback_result(success, host_error, "failed to register storage extension")?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_resolve_attach(
    path: RustExtString,
    host: *const RustExtAttachHost,
    userdata: *mut c_void,
    out: *mut RustExtAttachConfig,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(
        err,
        "failed to resolve Superhuman Docs attach config",
        || write_out(out, block_on_result(resolve_attach(path, host, userdata))?),
    )
}
