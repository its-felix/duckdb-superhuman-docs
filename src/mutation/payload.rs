use serde_json::{json, Value};
use superhuman_docs_async::operations;

use crate::ffi::*;
use crate::model::column_from_handle;

fn input_value_json(value: RustExtInputValue) -> Result<Value, String> {
    match value.value_type {
        0 => Ok(Value::Null),
        1 => Ok(Value::Bool(value.bool_value)),
        2 => Ok(json!(value.int_value)),
        3 => Ok(json!(value.uint_value)),
        4 => Ok(json!(value.double_value)),
        5 => Ok(Value::String(value.string_value.as_str().to_string())),
        RUST_EXT_INPUT_JSON => serde_json::from_str(value.string_value.as_str())
            .map_err(|error| format!("invalid composite DuckDB value: {error}")),
        _ => Err(format!(
            "unsupported DuckDB input value type: {}",
            value.value_type
        )),
    }
}

fn object_field(value: &Value, fields: &[&str]) -> Option<Value> {
    let object = value.as_object()?;
    fields
        .iter()
        .filter_map(|field| object.get(*field))
        .find(|field| !field.is_null() && field.as_str() != Some(""))
        .cloned()
}

fn simple_cell_value(format_type: &str, value: &Value) -> Result<Value, String> {
    if value.is_null() {
        return Err("Superhuman Docs cell arrays cannot contain NULL".to_string());
    }
    let normalized_type = format_type.to_ascii_lowercase();
    let simple = match normalized_type.as_str() {
        // The rows API accepts only primitive values (or arrays of primitives) for edits.
        // Rich JSON-LD objects are a read representation and must be reduced to the
        // primitive that the destination column parser understands.
        "currency" => object_field(value, &["amount"]),
        "image" => object_field(value, &["url"]),
        "person" => object_field(value, &["email", "name"]),
        "link" | "hyperlink" => object_field(value, &["url"]),
        "lookup" => object_field(value, &["rowId", "name"]),
        _ => Some(value.clone()),
    }
    .ok_or_else(|| format!("{format_type} value is missing its writable field"))?;

    if simple.is_object() || simple.is_array() || simple.is_null() {
        return Err(format!(
            "{format_type} values cannot be serialized to a Superhuman Docs cell primitive"
        ));
    }
    Ok(simple)
}

fn scalar_value(value: Value) -> Result<operations::ScalarValue, String> {
    match value {
        Value::Bool(value) => Ok(operations::ScalarValue::Boolean(value)),
        Value::Number(value) => value
            .as_f64()
            .map(operations::ScalarValue::Number)
            .ok_or_else(|| {
                format!("number cannot be represented by the Superhuman Docs SDK: {value}")
            }),
        Value::String(value) => Ok(operations::ScalarValue::Text(value)),
        value => Err(format!(
            "Superhuman Docs cell values must be strings, numbers, or booleans, received {value}"
        )),
    }
}

fn cell_value(
    column: &crate::model::SuperhumanDocsColumn,
    value: RustExtInputValue,
) -> Result<operations::Value, String> {
    let value = input_value_json(value)?;
    if column.is_array {
        let values = value
            .as_array()
            .ok_or_else(|| format!("{} expects an array value, received {}", column.name, value))?;
        return values
            .iter()
            .map(|value| simple_cell_value(&column.format_type, value))
            .map(|value| value.and_then(scalar_value))
            .collect::<Result<Vec<_>, _>>()
            .map(operations::Value::FlatList);
    }
    scalar_value(simple_cell_value(&column.format_type, &value)?).map(operations::Value::Scalar)
}

fn write_cells(
    columns: &[RustExtWriteColumn],
    values: &[RustExtInputValue],
    omit_nulls: bool,
) -> Result<operations::RowEdit, String> {
    let mut cells = Vec::new();
    for (column, value) in columns.iter().zip(values.iter()) {
        if column.capabilities & RUST_EXT_COLUMN_EDITABLE == 0 {
            continue;
        }
        if value.value_type == RUST_EXT_INPUT_NULL {
            if omit_nulls {
                continue;
            }
            return Err("Superhuman Docs does not support updating a cell to NULL".to_string());
        }
        let column = column_from_handle(column.handle)?;
        cells.push(operations::CellEdit {
            column: column.id.clone(),
            value: cell_value(column, *value)?,
        });
    }
    Ok(operations::RowEdit { cells })
}

pub(crate) fn insert_payload(
    columns: &[RustExtWriteColumn],
    values: &[RustExtInputValue],
    row_count: usize,
    value_column_count: usize,
    table_capabilities: u32,
) -> Result<operations::RowsUpsert, String> {
    if table_capabilities & RUST_EXT_TABLE_INSERT == 0 {
        return Err("insert is unsupported for this table".to_string());
    }
    if value_column_count < columns.len() || values.len() < row_count * value_column_count {
        return Err("invalid insert value shape".to_string());
    }
    let mut rows = Vec::with_capacity(row_count);
    for row_index in 0..row_count {
        let start = row_index * value_column_count;
        rows.push(write_cells(
            columns,
            &values[start..start + value_column_count],
            true,
        )?);
    }
    Ok(operations::RowsUpsert {
        rows,
        key_columns: None,
    })
}

pub(crate) fn update_payload(
    columns: &[RustExtWriteColumn],
    values: &[RustExtInputValue],
    table_capabilities: u32,
) -> Result<operations::RowUpdate, String> {
    if table_capabilities & RUST_EXT_TABLE_UPDATE == 0 {
        return Err("update is unsupported for this table".to_string());
    }
    if columns.len() != values.len() {
        return Err("invalid update value shape".to_string());
    }
    Ok(operations::RowUpdate {
        row: write_cells(columns, values, false)?,
    })
}

pub(crate) fn build_equality_query(
    column_id: &str,
    column_name: &str,
    value: RustExtInputValue,
) -> Result<(RustExtString, RustExtString), String> {
    if value.value_type == 0 {
        return Err("null query value".to_string());
    }
    let literal = input_value_json(value)?.to_string();
    let description = match value.value_type {
        1 => value.bool_value.to_string(),
        2 => value.int_value.to_string(),
        3 => value.uint_value.to_string(),
        4 => value.double_value.to_string(),
        5 => value.string_value.as_str().to_string(),
        _ => "NULL".to_string(),
    };
    Ok((
        alloc_string(&format!("{column_id}:{literal}")),
        alloc_string(&format!("{column_name} = {description}")),
    ))
}
