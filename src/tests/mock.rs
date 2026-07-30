use super::*;

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_scan_metadata_and_dml() {
    let server = MockSuperhumanDocsServer::start();
    let table = "superhuman_docs_doc.main.\"Tasks\"";
    let sql = format!(
        "LOAD {};\
         ATTACH 'mock-doc' AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {}, INCLUDE_ROW_METADATA true);\
         SELECT \"Name\", \"Done\", \"Amount\" FROM {table} ORDER BY \"Name\";\
         SELECT \"Name\" FROM {table} WHERE \"Name\" = 'Alpha';\
         SELECT \"Name\", createdAt FROM {table} ORDER BY createdAt LIMIT 1;\
         INSERT INTO {table} (\"Name\", \"Done\", \"Amount\") VALUES ('Gamma', false, 3.5);\
         UPDATE {table} SET \"Done\" = false, \"Amount\" = 4.5 WHERE \"Name\" = 'Alpha';\
         DELETE FROM {table} WHERE \"Name\" = 'Beta';",
        sql_literal(extension_path()),
        sql_literal(&server.base_url())
    );
    let output = run_duckdb(&sql);
    assert!(output.contains("Alpha,true,1.25"), "{output}");
    assert!(output.contains("Beta,false,2.5"), "{output}");
    assert!(output.contains("Alpha,2024-01-01"), "{output}");

    let requests = server.requests();
    assert!(
        requests.iter().any(|request| request.method == "GET"
            && request.path == "/docs/mock-doc/tables/tbl1/rows"
            && request.query.contains("valueFormat=rich")),
        "expected rich row values, got {requests:#?}"
    );
    assert!(
        requests.iter().any(|request| request.method == "POST"
            && request.path == "/docs/mock-doc/tables/tbl1/rows"
            && request.body.contains("\"Gamma\"")),
        "expected insert request, got {requests:#?}"
    );
    assert!(
        requests.iter().any(|request| request.method == "GET"
            && request.path == "/docs/mock-doc/tables/tbl1/rows"
            && request.query.contains("query=c_name%3A%22Alpha%22")),
        "expected equality filter pushdown for Alpha, got {requests:#?}"
    );
    assert!(
        requests.iter().any(|request| request.method == "PUT"
            && request.path == "/docs/mock-doc/tables/tbl1/rows/r1"
            && request.body.contains("\"value\":4.5")),
        "expected update request, got {requests:#?}"
    );
    assert!(
        requests.iter().any(|request| request.method == "DELETE"
            && request.path == "/docs/mock-doc/tables/tbl1/rows"
            && request.body.contains("\"r2\"")),
        "expected delete request, got {requests:#?}"
    );
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_waits_for_mutations() {
    let server = MockSuperhumanDocsServer::start();
    let table = "superhuman_docs_doc.main.\"Tasks\"";
    let sql = format!(
        "LOAD {};
         ATTACH 'mock-doc' AS superhuman_docs_doc
             (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {},
              WAIT_FOR_MUTATIONS true, MUTATION_TIMEOUT_SECONDS 2,
              ALLOW_MUTATION_WARNINGS false);
         INSERT INTO {table} (\"Name\", \"Done\", \"Amount\") VALUES ('Wait', false, 3.5);
         UPDATE {table} SET \"Done\" = false WHERE \"Name\" = 'Alpha';
         DELETE FROM {table} WHERE \"Name\" = 'Beta';",
        sql_literal(extension_path()),
        sql_literal(&server.base_url())
    );
    run_duckdb(&sql);
    let requests = server.requests();
    assert!(requests.iter().any(|request| {
        request.method == "GET" && request.path == "/mutationStatus/wait-request"
    }));
    assert!(requests.iter().any(|request| {
        request.method == "GET" && request.path == "/mutationStatus/update-request"
    }));
    assert!(requests.iter().any(|request| {
        request.method == "GET" && request.path == "/mutationStatus/delete-request"
    }));
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_applies_mutation_warning_policy() {
    let server = MockSuperhumanDocsServer::start();
    let table = "superhuman_docs_doc.main.\"Tasks\"";
    let setup = format!(
        "LOAD {};
         ATTACH 'mock-doc' AS superhuman_docs_doc
             (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {},
              WAIT_FOR_MUTATIONS true, MUTATION_TIMEOUT_SECONDS 2);",
        sql_literal(extension_path()),
        sql_literal(&server.base_url())
    );
    let (success, output) = run_duckdb_command_after_setup(
        &setup,
        &format!(
            "INSERT INTO {table} (\"Name\", \"Done\", \"Amount\") VALUES ('Warn', false, 3.5);"
        ),
    );
    assert!(!success, "{output}");
    assert!(output.contains("completed with a warning"), "{output}");
    assert!(output.contains("cannot be rolled back"), "{output}");

    let server = MockSuperhumanDocsServer::start();
    let sql = format!(
        "LOAD {};
         ATTACH 'mock-doc' AS superhuman_docs_doc
             (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {},
              WAIT_FOR_MUTATIONS true, MUTATION_TIMEOUT_SECONDS 2,
              ALLOW_MUTATION_WARNINGS true);
         INSERT INTO {table} (\"Name\", \"Done\", \"Amount\") VALUES ('Warn', false, 3.5);",
        sql_literal(extension_path()),
        sql_literal(&server.base_url())
    );
    run_duckdb(&sql);
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_reports_mutation_timeout() {
    let server = MockSuperhumanDocsServer::start();
    let table = "superhuman_docs_doc.main.\"Tasks\"";
    let setup = format!(
        "LOAD {};
         ATTACH 'mock-doc' AS superhuman_docs_doc
             (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {},
              WAIT_FOR_MUTATIONS true, MUTATION_TIMEOUT_SECONDS 1);",
        sql_literal(extension_path()),
        sql_literal(&server.base_url())
    );
    let (success, output) = run_duckdb_command_after_setup(
        &setup,
        &format!(
            "INSERT INTO {table} (\"Name\", \"Done\", \"Amount\") VALUES ('Timeout', false, 3.5);"
        ),
    );
    assert!(!success, "{output}");
    assert!(
        output.contains("did not complete within 1 seconds"),
        "{output}"
    );
    assert!(output.contains("may occur later"), "{output}");
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_attaches_browser_urls() {
    for prefix in ["coda:", "superhuman:", "superhuman-docs:"] {
        let server = MockSuperhumanDocsServer::start();
        let browser_url = format!("{prefix}https://coda.io/d/Mock_dmock-doc/Table_table-link");
        let sql = format!(
            "LOAD {};
             ATTACH {} AS superhuman_docs_doc
                 (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {});
             SELECT count(*) FROM superhuman_docs_doc.main.\"Tasks\";",
            sql_literal(extension_path()),
            sql_literal(&browser_url),
            sql_literal(&server.base_url())
        );
        let output = run_duckdb(&sql);
        assert!(output.lines().any(|line| line == "2"), "{output}");
        let requests = server.requests();
        assert!(requests.iter().any(|request| {
            request.method == "GET"
                && request.path == "/resolveBrowserLink"
                && request.query.contains("degradeGracefully=false")
                && request.query.contains("table-link")
        }));
        assert!(requests
            .iter()
            .any(|request| { request.method == "GET" && request.path == "/docs/mock-doc/tables" }));
    }

    let direct_server = MockSuperhumanDocsServer::start();
    let direct_sql = format!(
        "LOAD {};
         ATTACH 'mock-doc' AS superhuman_docs_doc
             (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {});
         SELECT count(*) FROM superhuman_docs_doc.main.\"Tasks\";",
        sql_literal(extension_path()),
        sql_literal(&direct_server.base_url())
    );
    run_duckdb(&direct_sql);
    assert!(!direct_server
        .requests()
        .iter()
        .any(|request| request.path == "/resolveBrowserLink"));
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_rejects_non_doc_browser_resources() {
    let server = MockSuperhumanDocsServer::start();
    let setup = format!("LOAD {};", sql_literal(extension_path()));
    let sql = format!(
        "ATTACH 'coda:https://coda.io/folders/folder-link' AS superhuman_docs_doc
             (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {});",
        sql_literal(&server.base_url())
    );
    let (success, output) = run_duckdb_command_after_setup(&setup, &sql);
    assert!(!success, "{output}");
    assert!(output.contains("not contained by a document"), "{output}");
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_token_env_for_attach() {
    let server = MockSuperhumanDocsServer::start();
    let env_name = "DUCKDB_SUPERHUMAN_DOCS_MOCK_API_TOKEN";
    let sql = format!(
        "LOAD {};
         ATTACH 'mock-doc' AS superhuman_docs_attach_env
             (TYPE superhuman_docs, TOKEN_ENV {}, API_BASE {});
         SELECT count(*) FROM superhuman_docs_attach_env.main.\"Tasks\";",
        sql_literal(extension_path()),
        sql_literal(env_name),
        sql_literal(&server.base_url())
    );
    let output = run_duckdb_with_env(&sql, env_name, "mock-token");
    assert!(output.lines().any(|line| line == "2"), "{output}");
    assert!(
        server.requests().iter().all(|request| request
            .headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer mock-token"))),
        "resolved environment token was not used for every request: {:#?}",
        server.requests()
    );
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_wide_types() {
    let server = MockSuperhumanDocsServer::start();
    let table = "superhuman_docs_doc.main.\"Wide Types\"";
    let sql = format!(
        "LOAD {};\
         ATTACH 'mock-doc' AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {});\
         SELECT column_name, data_type FROM information_schema.columns \
         WHERE table_catalog = 'superhuman_docs_doc' AND table_schema = 'main' AND table_name = 'Wide Types' \
         ORDER BY ordinal_position;\
         SELECT \"Checkbox\", \"Text\", \"Email\", \"Select\", \
                \"Number\", \"Percent\", \"Slider\", \"Progress\", \"Scale\", \
                \"Date\", \"DateTime\", \"Time\", epoch(\"Duration\"), \
                \"Currency\".currency, \"Currency\".amount, \
                \"Image\".name, \"Image\".url, \"Image\".height, \"Image\".width, \"Image\".status, \
                \"Person\".name, \"Person\".email, \
                \"Hyperlink\".name, \"Hyperlink\".url, \
                \"Lookup\".name, \"Lookup\".url, \"Lookup\".tableId, \"Lookup\".tableUrl, \"Lookup\".rowId, \
                CAST(\"Other\" AS VARCHAR), CAST(\"MultiSelect\" AS VARCHAR), \
                list_transform(\"Durations\", lambda value: epoch(value)), \
                list_transform(\"Currencies\", lambda value: value.currency), CAST(\"Others\" AS VARCHAR) \
         FROM {table};
         INSERT INTO {table} (\"Number\", \"Percent\", \"Slider\", \"Progress\", \"Scale\", \"Currency\", \"Image\", \"Person\", \"Hyperlink\", \"Lookup\", \"MultiSelect\", \"Currencies\")
         VALUES (
             123.45, 0.6667, 25, 0.4, 4,
             struct_pack(currency := 'EUR', amount := 10.0),
             struct_pack(name := 'photo.png', url := 'https://example.com/photo.png', height := 480, width := 640, status := 'live'),
             struct_pack(name := 'Ada Lovelace', email := 'ada@example.com'),
             struct_pack(name := 'Example', url := 'https://example.com'),
             struct_pack(name := 'Referenced row', url := 'https://coda.io/row', tableId := 'tbl-related', tableUrl := 'https://coda.io/table', rowId := 'row-related'),
             ['One', 'Two'],
             [struct_pack(currency := 'USD', amount := 12.34), struct_pack(currency := 'EUR', amount := 56.78)]
         );",
        sql_literal(extension_path()),
        sql_literal(&server.base_url()),
    );
    let output = run_duckdb(&sql);
    for expected in [
        "Checkbox,BOOLEAN",
        "Text,VARCHAR",
        "Email,VARCHAR",
        "Select,VARCHAR",
        "Number,\"DECIMAL(38,20)\"",
        "Percent,\"DECIMAL(38,20)\"",
        "Slider,\"DECIMAL(38,20)\"",
        "Progress,\"DECIMAL(38,20)\"",
        "Scale,\"DECIMAL(38,20)\"",
        "Date,DATE",
        "DateTime,TIMESTAMP WITH TIME ZONE",
        "Time,TIME",
        "Duration,INTERVAL",
        "Currency,\"STRUCT(currency VARCHAR, amount DECIMAL(38,20))\"",
        "Image,\"STRUCT(\"\"name\"\" VARCHAR, url VARCHAR, height DOUBLE, width DOUBLE, status VARCHAR)\"",
        "Person,\"STRUCT(\"\"name\"\" VARCHAR, email VARCHAR)\"",
        "Hyperlink,\"STRUCT(\"\"name\"\" VARCHAR, url VARCHAR)\"",
        "Lookup,\"STRUCT(\"\"name\"\" VARCHAR, url VARCHAR, tableId VARCHAR, tableUrl VARCHAR, rowId VARCHAR)\"",
        "Other,JSON",
        "MultiSelect,VARCHAR[]",
        "Durations,INTERVAL[]",
        "Currencies,\"STRUCT(currency VARCHAR, amount DECIMAL(38,20))[]\"",
        "Others,JSON[]",
        "true,Alpha,ada@example.com,Open",
        "123456789012345678.12345678901234567890",
        "2024-01-02,2024-01-02 03:04:05+00,03:04:05,43200.0,USD,12.34000000000000000000",
        "photo.png,https://example.com/photo.png,480.0,640.0,live",
        "Ada Lovelace,ada@example.com,Example,https://example.com",
        "Referenced row,https://coda.io/row,tbl-related,https://coda.io/table,row-related",
        "nested",
        "One",
        "[43200.0, 86400.0]",
        "[USD, EUR]",
    ] {
        assert!(
            output.contains(expected),
            "expected wide-type output to contain '{expected}', got:\n{output}"
        );
    }
    assert!(
        server.requests().iter().any(|request| {
            request.method == "GET"
                && request.path == "/docs/mock-doc/tables/tbl_wide/rows"
                && request.query.contains("valueFormat=rich")
        }),
        "wide table scan did not request rich values"
    );
    let request = server
        .requests()
        .into_iter()
        .find(|request| {
            request.method == "POST" && request.path == "/docs/mock-doc/tables/tbl_wide/rows"
        })
        .expect("wide table insert was not sent");
    assert!(request.query.contains("disableParsing=false"));
    let number = |value| serde_json::from_str::<Value>(value).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&request.body).unwrap(),
        json!({
            "rows": [{
                "cells": [
                    {"column": "c_number", "value": number("123.45")},
                    {"column": "c_percent", "value": number("0.6667")},
                    {"column": "c_slider", "value": number("25.0")},
                    {"column": "c_progress", "value": number("0.4")},
                    {"column": "c_scale", "value": number("4.0")},
                    {"column": "c_currency", "value": number("10.0")},
                    {"column": "c_image", "value": "https://example.com/photo.png"},
                    {"column": "c_person", "value": "ada@example.com"},
                    {"column": "c_hyperlink", "value": "https://example.com"},
                    {"column": "c_lookup", "value": "row-related"},
                    {"column": "c_multiselect", "value": ["One", "Two"]},
                    {"column": "c_currencies", "value": [number("12.34"), number("56.78")]},
                ]
            }]
        })
    );
}

#[test]
#[ignore]
fn duckdb_mock_superhuman_docs_rejects_explicit_transactions() {
    let server = MockSuperhumanDocsServer::start();
    let setup = format!(
        "LOAD {};\
         ATTACH 'mock-doc' AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN 'mock-token', API_BASE {});",
        sql_literal(extension_path()),
        sql_literal(&server.base_url())
    );
    let (success, output) = run_duckdb_command_after_setup(
        &setup,
        "BEGIN TRANSACTION; INSERT INTO superhuman_docs_doc.main.\"Tasks\" (\"Name\", \"Done\", \"Amount\") VALUES ('Txn', false, 9.0); ROLLBACK;",
    );
    assert!(
        !success
            && output.contains("Superhuman Docs does not support explicit DuckDB transactions"),
        "{output}\nrequests: {:#?}",
        server.requests()
    );
    assert!(
        !server
            .requests()
            .iter()
            .any(|request| request.method == "POST"),
        "transactional write should be rejected before HTTP mutation"
    );
}
