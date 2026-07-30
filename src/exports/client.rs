use std::ffi::c_void;

use crate::client::load_catalog;
use crate::ffi::*;
use crate::json::ffi::free_catalog;
use crate::model::{client_config, table_from_handle};
use crate::mutation::{delete_rows, insert_rows, update_rows};
use crate::platform::block_on_result;

#[no_mangle]
pub extern "C" fn rust_ext_client_load_catalog(
    config: RustExtClientConfig,
    out: *mut RustExtCatalog,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to load Superhuman Docs catalog", || {
        write_out(out, block_on_result(load_catalog(client_config(config)?))?)
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_client_insert_rows(
    config: RustExtClientConfig,
    table: *mut c_void,
    columns_ptr: *const RustExtWriteColumn,
    column_count: usize,
    values_ptr: *const RustExtInputValue,
    row_count: usize,
    value_column_count: usize,
    affected_count: *mut usize,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to insert Superhuman Docs rows", || {
        let columns = slice_from_raw_parts(columns_ptr, column_count);
        let values = slice_from_raw_parts(values_ptr, row_count * value_column_count);
        write_out(
            affected_count,
            block_on_result(insert_rows(
                client_config(config)?,
                table_from_handle(table)?,
                columns,
                values,
                row_count,
                value_column_count,
            ))?,
        )
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_client_update_rows(
    config: RustExtClientConfig,
    table: *mut c_void,
    row_ids_ptr: *const RustExtString,
    row_count: usize,
    columns_ptr: *const RustExtWriteColumn,
    column_count: usize,
    values_ptr: *const RustExtInputValue,
    affected_count: *mut usize,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to update Superhuman Docs rows", || {
        let row_ids = slice_from_raw_parts(row_ids_ptr, row_count);
        let columns = slice_from_raw_parts(columns_ptr, column_count);
        let values = slice_from_raw_parts(values_ptr, row_count * column_count);
        write_out(
            affected_count,
            block_on_result(update_rows(
                client_config(config)?,
                table_from_handle(table)?,
                row_ids,
                columns,
                values,
            ))?,
        )
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_client_delete_rows(
    config: RustExtClientConfig,
    table: *mut c_void,
    row_ids_ptr: *const RustExtString,
    count: usize,
    affected_count: *mut usize,
    err: *mut RustExtError,
) -> bool {
    ffi_bool(err, "failed to delete Superhuman Docs rows", || {
        let row_ids = slice_from_raw_parts(row_ids_ptr, count);
        write_out(
            affected_count,
            block_on_result(delete_rows(
                client_config(config)?,
                table_from_handle(table)?,
                row_ids,
            ))?,
        )
    })
}

#[no_mangle]
pub extern "C" fn rust_ext_free_catalog(catalog: RustExtCatalog) {
    free_catalog(catalog);
}
