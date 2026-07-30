use super::*;

pub(super) fn run_duckdb_success_case(
    resource: &str,
    credential: &str,
    endpoint: &str,
    table_name: &str,
) {
    let table = format!("superhuman_docs_doc.main.{}", sql_ident(table_name));
    let sql = format!(
        "LOAD {};\
         ATTACH {} AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN {}, API_BASE {}, WAIT_FOR_MUTATIONS true);\
         SELECT {}, {}, {} FROM {table} ORDER BY {};\
         SELECT * FROM {table} WHERE {} != '';\
         INSERT INTO {table} ({}, {}, {}) VALUES ('Gamma', false, 3.5);\
         UPDATE {table} SET {} = false, {} = 4.5 WHERE {} = 'Alpha';\
         DELETE FROM {table} WHERE {} = 'Beta';",
        sql_literal(extension_path()),
        sql_literal(resource),
        sql_literal(credential),
        sql_literal(endpoint),
        sql_ident("Name"),
        sql_ident("Done"),
        sql_ident("Amount"),
        sql_ident("Name"),
        sql_ident("Name"),
        sql_ident("Name"),
        sql_ident("Done"),
        sql_ident("Amount"),
        sql_ident("Done"),
        sql_ident("Amount"),
        sql_ident("Name"),
        sql_ident("Name"),
    );
    let output = run_duckdb(&sql);
    assert!(
        output.contains("Alpha") && output.contains("Beta"),
        "expected initial SELECT output to include seed rows, got:\n{output}"
    );
}

pub(super) fn run_duckdb_metadata_case(
    resource: &str,
    credential: &str,
    endpoint: &str,
    table_name: &str,
) {
    let table = format!("superhuman_docs_doc.main.{}", sql_ident(table_name));
    let sql = format!(
        "LOAD {};\
         ATTACH {} AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN {}, API_BASE {}, INCLUDE_ROW_METADATA true);\
         SELECT column_name, data_type \
         FROM information_schema.columns \
         WHERE table_catalog = 'superhuman_docs_doc' \
           AND table_schema = 'main' \
           AND table_name = {} \
           AND column_name IN ('createdAt', 'updatedAt') \
         ORDER BY column_name;\
         SELECT {}, typeof(createdAt), typeof(updatedAt) \
         FROM {table} \
         WHERE {} = 'Alpha';",
        sql_literal(extension_path()),
        sql_literal(resource),
        sql_literal(credential),
        sql_literal(endpoint),
        sql_literal(table_name),
        sql_ident("Name"),
        sql_ident("Name"),
    );
    let output = run_duckdb(&sql);
    for expected in [
        "createdAt,TIMESTAMP WITH TIME ZONE",
        "updatedAt,TIMESTAMP WITH TIME ZONE",
        "Alpha,TIMESTAMP WITH TIME ZONE,TIMESTAMP WITH TIME ZONE",
    ] {
        assert!(
            output.contains(expected),
            "expected metadata output line '{expected}', got:\n{output}"
        );
    }
}
