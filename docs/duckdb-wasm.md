# Custom DuckDB-Wasm bundles

The `duckdb-wasm` submodule pins DuckDB-Wasm so `superhuman_docs` can be built into the `mvp`, `eh`, and `coi`
(`wasm_threads`) bundles. The build applies `patches/duckdb-wasm.patch` to an isolated worktree under `build/`, leaving
the checked-in submodule clean. No extension side module and no `httpfs` transport are used. Emscripten Fetch handles
HTTP and Asyncify suspends the synchronous DuckDB ABI while a request is in flight.

## Build

Install the Rust `rust-src` component, activate Emscripten 4.0.23, then run:

```sh
git submodule update --init duckdb-wasm
./scripts/build_duckdb_wasm.sh
```

The script creates a detached worktree at the revision pinned by the submodule, applies DuckDB-Wasm's upstream DuckDB
patches and `patches/duckdb-wasm.patch`, builds all three release bundles, and creates the JavaScript package. Pass a
worktree path as the first argument to keep the generated tree somewhere other than `build/duckdb-wasm-superhuman`.

## Browser verification

The patch adds a same-origin mock API, COOP/COEP response headers, and browser coverage for catalog loading, paginated
scans, insert/update/bulk-delete, the exact `DELETE` JSON body, mutation polling, HTTP/network failures, and continued
worker use after errors. Run the suite once per bundle selection after building the package.

```sh
for variant in mvp eh coi; do
  DUCKDB_WASM_TEST_BUNDLE="$variant" \
    DUCKDB_WASM_SUPERHUMAN_ONLY=1 \
    CHROME_BIN="/path/to/Google Chrome" \
    yarn --cwd build/duckdb-wasm-superhuman workspace @duckdb/duckdb-wasm test:chrome
done
```

Use the Node.js 22 runtime bundled with Emscripten 4.0.23; the pinned DuckDB-Wasm toolchain is not compatible with newer
Node.js releases.

## Release measurements

Measurements were taken on macOS with Chrome 150 against the pinned checkout and Emscripten 4.0.23. The custom release
bundle sizes are:

| Bundle | Bytes |
| --- | ---: |
| `mvp` | 54,325,439 |
| `eh` | 122,231,771 |
| `coi` | 47,688,695 |

For an apples-to-apples MVP comparison, the same patched checkout built without `superhuman_docs`, Fetch, or Asyncify was
30,413,660 bytes. The custom MVP bundle is 23,911,779 bytes larger (+78.62%). This is the combined cost of the built-in
extension, Fetch runtime, and broad Asyncify instrumentation; those pieces cannot be separated in a runnable extension
bundle because its synchronous ABI depends on Asyncify.

A local-query benchmark warmed the connection with 10 queries and measured the median of five rounds of 100 sequential
`SELECT sum(i) FROM range(10000)` queries. The upstream MVP baseline took 64.005 ms per round (0.640 ms/query); the custom
MVP took 133.535 ms (1.335 ms/query), a 69.530 ms round increase (+108.63%, or 0.695 ms/query). Network-bound extension
queries are dominated by HTTP latency, so this measurement intentionally exposes the fixed cost of broad Asyncify on
short local queries.
