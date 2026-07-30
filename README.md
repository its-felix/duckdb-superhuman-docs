# DuckDB Superhuman Docs Extension

This extension attaches a Superhuman Docs document as a DuckDB database. Superhuman Docs tables are exposed as DuckDB tables.

## Usage

```sql
INSTALL superhuman_docs;
LOAD superhuman_docs;

CREATE SECRET superhuman_docs_token (
    TYPE superhuman_docs,
    TOKEN 'superhuman-docs-api-token'
);

ATTACH 'doc-id' AS superhuman_docs_doc (TYPE superhuman_docs);

SELECT * FROM superhuman_docs_doc.main."Tasks";
INSERT INTO superhuman_docs_doc.main."Tasks" ("Task", "Done") VALUES ('Ship extension', false);
UPDATE superhuman_docs_doc.main."Tasks" SET "Done" = true WHERE "Task" = 'Ship extension';
DELETE FROM superhuman_docs_doc.main."Tasks" WHERE "Task" = 'Ship extension';
```

You can attach a Superhuman Docs browser URL instead of extracting its document ID:

```sql
ATTACH 'coda:https://coda.io/d/Launch-Status_dAbCDeFGH/Tasks_su123'
    AS superhuman_docs_doc (TYPE superhuman_docs);
```

Prefix browser URLs with `coda:`, `superhuman:`, or `superhuman-docs:` so DuckDB routes the resource to this storage
extension instead of treating it as a remote database file. Keep `TYPE superhuman_docs` in the attach options. The
legacy `superhuman_docs:` prefix remains supported for compatibility.

Document URLs and URLs for resources contained by a document, such as pages, tables, views, rows, columns, formulas,
and controls, attach the entire containing document. Raw document IDs do not make an additional URL-resolution
request. URL resolution requires an explicit `TOKEN`/`TOKEN_ENV`, a general `superhuman_docs:` secret, or a canonical
URL containing a document ID with a matching document-scoped secret. Deleted or non-document resources are rejected.

When a token is provided to `CREATE SECRET`, the extension validates it immediately with the Superhuman Docs `whoami` operation.
Secret creation succeeds only when that request returns HTTP 200; the response body is ignored.

You can also pass credentials at attach time:

```sql
ATTACH 'doc-id' AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN 'superhuman-docs-api-token');
```

To read the API token from an environment variable, use `TOKEN_ENV` with the variable's name:

```sql
CREATE SECRET superhuman_docs_token (
    TYPE superhuman_docs,
    TOKEN_ENV 'SUPERHUMAN_DOCS_API_TOKEN'
);

ATTACH 'doc-id' AS superhuman_docs_doc (TYPE superhuman_docs, TOKEN_ENV 'SUPERHUMAN_DOCS_API_TOKEN');
```

The environment variable is read eagerly when `CREATE SECRET` or `ATTACH` runs. For a secret, the resolved value is
stored as the token; the environment variable name is not retained. `TOKEN` and `TOKEN_ENV` cannot be specified
together.

To expose Superhuman Docs' row metadata as table columns, enable `INCLUDE_ROW_METADATA` when attaching:

```sql
ATTACH 'doc-id' AS superhuman_docs_doc (
    TYPE superhuman_docs,
    TOKEN 'superhuman-docs-api-token',
    INCLUDE_ROW_METADATA true
);

SELECT "Task", createdAt, updatedAt FROM superhuman_docs_doc.main."Tasks";
```

With this option enabled, every Superhuman Docs table includes `createdAt` and `updatedAt` columns. Both columns are
`TIMESTAMP WITH TIME ZONE` values and are read-only.

Superhuman Docs writes are asynchronous by default. To wait until every API mutation reports completion, configure the
attached database:

```sql
ATTACH 'doc-id' AS superhuman_docs_doc (
    TYPE superhuman_docs,
    WAIT_FOR_MUTATIONS true,
    MUTATION_TIMEOUT_SECONDS 60,
    ALLOW_MUTATION_WARNINGS false
);
```

`WAIT_FOR_MUTATIONS` defaults to `false`, preserving the API's accepted-for-processing behavior. When waiting is
enabled, `MUTATION_TIMEOUT_SECONDS` defaults to 60 and applies separately to each remote mutation. It must be a positive
integer. A completed mutation with a warning fails the statement unless `ALLOW_MUTATION_WARNINGS` is true. The timeout
and warning options may only be specified when waiting is enabled.

A timeout or status-check error leaves the remote outcome unknown: the mutation may still complete later. A warning
error means the remote mutation completed with a caveat. Neither case can be rolled back, so retry only after checking
the document. A transient 404 from the mutation-status endpoint is treated as pending because newly accepted mutation
IDs can take a short time to become visible; other HTTP errors fail immediately.

The initial version intentionally exposes only Superhuman Docs tables. DDL is not supported: the extension does not
create, drop, or alter Superhuman Docs tables.

## Column Types

The extension requests rich row values and maps scalar Superhuman Docs column formats to DuckDB as follows:

| Superhuman Docs format | DuckDB type |
| --- | --- |
| `checkbox` | `BOOLEAN` |
| `text`, `email`, `select` | `VARCHAR` |
| `number`, `percent`, `slider` (including progress display), `scale` | `DECIMAL(38, 20)` |
| `date` | `DATE` |
| `dateTime` | `TIMESTAMP WITH TIME ZONE` |
| `time` | `TIME` |
| `duration` | `INTERVAL` |
| `currency` | `STRUCT(currency VARCHAR, amount DECIMAL(38, 20))` |
| `image` | `STRUCT(name VARCHAR, url VARCHAR, height DOUBLE, width DOUBLE, status VARCHAR)` |
| `person` | `STRUCT(name VARCHAR, email VARCHAR)` |
| `link` | `STRUCT(name VARCHAR, url VARCHAR)` |
| `lookup` | `STRUCT(name VARCHAR, url VARCHAR, tableId VARCHAR, tableUrl VARCHAR, rowId VARCHAR)` |

Array-valued columns use the same mapping for their elements and expose a DuckDB array type; for example, an array
`duration` column becomes `INTERVAL[]`. A scalar `select` exposes its selected value as `VARCHAR`. Unsupported scalar
formats map to `JSON`, and arrays of unsupported values become `JSON[]`.

The API accepts only primitive cell values (or arrays of primitives) when rows are inserted or updated. The extension
therefore converts DuckDB's rich structs back to the primitive understood by the destination column:

Numeric formats (`number`, `percent`, `slider`, progress-style sliders, and `scale`) are sent as JSON numbers while
preserving DuckDB's exact decimal representation. Quoted decimal strings are not used because the UI treats them as
invalid values even when their text can be rendered as a number.

| DuckDB rich type | Value sent when writing |
| --- | --- |
| `currency` | `amount` (the destination column determines the currency format) |
| `image` | `url` |
| `person` | `email`, falling back to `name` |
| `link`, `hyperlink` | `url` |
| `lookup` | `rowId`, falling back to `name` |

Array-valued columns are sent as JSON arrays after applying the same conversion to every element. The JSON-LD objects
returned by `valueFormat=rich` are read representations; sending those objects directly would create invalid cell
values.

## Repository Layout

- `src/` contains the Rust extension implementation, C ABI exports, API parsing, and Superhuman Docs behavior.
- `src/cpp/` contains the DuckDB C++ storage extension plumbing.
- `src/include/` contains the public C ABI and C++ bridge headers.
- `test/sql/` contains DuckDB sqllogictest files loaded through `extension_config.cmake`.
- `Cargo.toml` builds the Rust static library linked by the DuckDB C++ extension.
- `duckdb-wasm/` is a pinned upstream submodule used to produce browser bundles; see `docs/duckdb-wasm.md`.

## Testing

```sh
cargo test
make release
make test
```

`cargo test` runs the Rust unit tests for the ABI implementation, request body generation, response parsing, and the
native async HTTP transport. Transport coverage includes all supported request verbs, JSON bodies on `DELETE`, empty
and non-2xx responses, malformed HTTP, timeouts, authentication headers, and one-attempt behavior. The DuckDB build
invokes Cargo to produce a Rust static library and links it into the loadable extension.

The DuckDB extension Makefile targets remain available:

```sh
make verify
make test_superhuman_docs_http_mock
make test_superhuman_docs_http_real
```

`make verify` runs the Rust tests and a release build. `make test_superhuman_docs_http_mock` runs the Rust tests plus
ignored DuckDB integration tests against a local mock HTTP server through `API_BASE`. It expects
`build/release/duckdb` and `build/release/extension/superhuman_docs/superhuman_docs.duckdb_extension` to exist; run
`make release` first when those binaries are missing or stale. `make test_superhuman_docs_http_real` runs a release
build plus the ignored live Superhuman Docs API integration test.

The real Superhuman Docs integration tests only require `SUPERHUMAN_DOCS_TEST_API_TOKEN` and
`SUPERHUMAN_DOCS_TEST_DOC_ID`. `make test_superhuman_docs_http_real` exports a local `.env` file before running the
tests, matching the same environment variables provided by CI. The harness creates and deletes a temporary per-run page
for the basic read/write smoke test. The configured document must also contain the captured
`duckdb_superhuman_docs_wide_types` and `duckdb_superhuman_docs_wide_types_fixture` tables; their IDs are discovered from
the catalog and are not environment variables.

Superhuman Docs' page-content API imports generated table columns as text and does not expose an operation for changing built-in
column formats. The mutable wide table test validates its captured 26-column schema, inserts values across all scalar
writable formats plus canvas text, waits for Superhuman Docs' asynchronous mutation, reads the values through DuckDB, and
deletes its test row. The separate fixture test issues only `SELECT` statements and covers all 26 columns using stable rich
values from ten rows, including currency, people, links, lookups, images, canvas JSON, dates, times, durations, and
percentages. The mocked
HTTP test retains deterministic coverage for non-empty array values and other API shapes that the live fixture leaves
empty.

## Notes

- Superhuman Docs row IDs are surfaced internally as DuckDB's virtual `rowid` and are used for updates and deletes.
- Superhuman Docs row metadata is omitted by default. Use `INCLUDE_ROW_METADATA true` on `ATTACH` to include `createdAt` and
  `updatedAt` columns.
- Superhuman Docs writes are asynchronous unless `WAIT_FOR_MUTATIONS true` is set on `ATTACH`. Without it, DML reports
  rows accepted by the API rather than rows fully materialized in the document.
- Explicit DuckDB transactions are not supported for attached Superhuman Docs databases. Use autocommit statements so the extension
  does not imply rollback semantics that Superhuman Docs cannot provide.
- Unsupported Superhuman Docs values use DuckDB's `JSON` type; all array-valued columns preserve their mapped inner type.
- API routing and request construction use the `superhuman-docs-async` Rust SDK resource clients. The existing
  blocking `superhuman-docs` crate remains available as a separate library.
  Native builds use a shared Tokio runtime and a shared Reqwest client with Rustls, a 30-second request timeout, and no
  automatic retries. Browser Emscripten builds use Emscripten Fetch through the extension's C++ shim. DuckDB-facing C
  exports remain synchronous: native builds block on Tokio, while Emscripten builds yield through Asyncify.

### Rust bridge boundary

The C++ sources under `src/cpp/rust_bridge_*` and `src/include/rust_bridge_*` are a generic DuckDB-to-Rust storage bridge. Extension policy stays on the Rust side of `rust_bridge_extension.h`:

- Rust registers secret parameters and resolves secret inputs into generic key/value results; C++ does not know token names, environment-variable behavior, validation rules, or default scopes.
- Attach, table, column, and scan-row implementation data are Rust-owned opaque handles. C++ only retains generic names, DuckDB logical type strings, and capability flags.
- Rust supplies complete DuckDB physical type declarations and optional logical aliases, which the bridge parses without an attached `ClientContext`; C++ contains no extension-specific rich-value layouts or source-format tags.
- Backend pagination limits and row/value interpretation remain in Rust. The bridge only implements DuckDB planning, value transfer, ownership, and lifecycle mechanics.
