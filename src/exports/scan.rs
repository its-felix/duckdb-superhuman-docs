use std::ffi::c_void;

use crate::client::ScanHandle;
use crate::ffi::*;
use crate::json::ffi::free_scan_batch;
use crate::model::{client_config, column_from_handle, row_from_handle, table_from_handle};
use crate::platform::block_on_result;
use crate::scan::scan_value;

#[no_mangle]
pub extern "C" fn rust_ext_scan_value(
    column: *mut c_void,
    row: *mut c_void,
    out: *mut RustExtScanValue,
) -> bool {
    let column = match column_from_handle(column) {
        Ok(column) => column,
        Err(_) => return false,
    };
    let row = match row_from_handle(row) {
        Ok(row) => row,
        Err(_) => return false,
    };
    write_out(out, scan_value(column, row)).is_ok()
}

#[no_mangle]
pub extern "C" fn rust_ext_free_scan_value(value: RustExtScanValue) {
    if value.value_owned {
        value.value.free();
    }
    for item in vec_from_raw_parts(value.array_values, value.array_count) {
        if !item.is_null {
            item.value.free();
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_ext_scan_open(
    config: RustExtClientConfig,
    table: *mut c_void,
    request: RustExtScanRequest,
    out: *mut *mut c_void,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to open Superhuman Docs scan", || {
        let handle = Box::new(ScanHandle::new(
            client_config(config)?,
            table_from_handle(table)?,
            request,
        )?);
        write_out(out, Box::into_raw(handle).cast::<c_void>())
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_scan_next(
    scan: *mut c_void,
    out: *mut RustExtScanBatch,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(
        err,
        "failed to fetch next Superhuman Docs scan batch",
        || {
            let handle = mut_from_raw(scan.cast::<ScanHandle>(), "scan handle")?;
            write_out(out, block_on_result(handle.next_batch())?)
        },
    )
}

#[no_mangle]
pub extern "C" fn rust_ext_scan_close(scan: *mut c_void) {
    if !scan.is_null() {
        drop(unsafe { Box::from_raw(scan.cast::<ScanHandle>()) });
    }
}

#[no_mangle]
pub extern "C" fn rust_ext_free_scan_batch(batch: RustExtScanBatch) {
    free_scan_batch(batch);
}
