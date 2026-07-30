PROJ_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

EXT_NAME=superhuman_docs
EXT_CONFIG=${PROJ_DIR}extension_config.cmake
EXT_FLAGS=-DCMAKE_CXX_STANDARD=17

DEFAULT_TEST_EXTENSION_DEPS=

include extension-ci-tools/makefiles/duckdb_extension.Makefile
include extension-ci-tools/makefiles/vcpkg.Makefile

.PHONY: wasm_pre_build_step
wasm_pre_build_step:
	rustup component add rust-src

.PHONY: verify
verify:
	cargo test
	$(MAKE) release

.PHONY: test_superhuman_docs_http_mock
test_superhuman_docs_http_mock:
	cargo test
	cargo test duckdb_mock_superhuman_docs -- --ignored --nocapture

.PHONY: test_superhuman_docs_http_real
test_superhuman_docs_http_real:
	$(MAKE) release
	set -a; [ ! -f .env ] || . ./.env; set +a; cargo test real_superhuman_docs_api -- --ignored --nocapture

.PHONY: duckdb_wasm_bundles
duckdb_wasm_bundles:
	./scripts/build_duckdb_wasm.sh
