use crate::ffi::*;
use crate::platform::block_on_result;
use crate::secret::{create_secret, free_secret};

#[no_mangle]
pub extern "C" fn rust_ext_create_secret(
    input: RustExtSecretCreateInput,
    out: *mut RustExtSecretCreateResult,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to create extension secret", || {
        write_out(out, block_on_result(create_secret(input))?)
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_free_secret(secret: RustExtSecretCreateResult) {
    free_secret(secret);
}
