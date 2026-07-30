#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXTENSION_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SUBMODULE_DIR="${EXTENSION_DIR}/duckdb-wasm"
PATCH_FILE="${EXTENSION_DIR}/patches/duckdb-wasm.patch"
CHECKOUT_DIR="${1:-${EXTENSION_DIR}/build/duckdb-wasm-superhuman}"
if [[ "${CHECKOUT_DIR}" != /* ]]; then
    CHECKOUT_DIR="${PWD}/${CHECKOUT_DIR}"
fi

if [ ! -e "${SUBMODULE_DIR}/.git" ]; then
    echo "duckdb-wasm submodule is not initialized; run: git submodule update --init duckdb-wasm" >&2
    exit 1
fi

if [ ! -f "${PATCH_FILE}" ]; then
    echo "DuckDB-Wasm overlay is missing: ${PATCH_FILE}" >&2
    exit 1
fi

if ! command -v emcc >/dev/null 2>&1; then
    echo "emcc is not available; activate Emscripten 4.0.23 first" >&2
    exit 1
fi
EMCC_VERSION="$(emcc --version)"
if [[ "${EMCC_VERSION}" != *"4.0.23"* ]]; then
    echo "this bundle is pinned to Emscripten 4.0.23" >&2
    exit 1
fi

RUST_SYSROOT="$(rustc --print sysroot)"
if [ ! -d "${RUST_SYSROOT}/lib/rustlib/src/rust/library/std" ]; then
    echo "rust-src is required; run: rustup component add rust-src" >&2
    exit 1
fi

SUBMODULE_COMMIT="$(git -C "${SUBMODULE_DIR}" rev-parse HEAD)"
if [ ! -e "${CHECKOUT_DIR}/.git" ]; then
    if [ -e "${CHECKOUT_DIR}" ]; then
        echo "${CHECKOUT_DIR} exists but is not a DuckDB-Wasm worktree" >&2
        exit 1
    fi
    mkdir -p "$(dirname "${CHECKOUT_DIR}")"
    git -C "${SUBMODULE_DIR}" worktree prune --expire now
    git -C "${SUBMODULE_DIR}" worktree add --detach "${CHECKOUT_DIR}" "${SUBMODULE_COMMIT}"
    git -C "${CHECKOUT_DIR}" submodule update --init --recursive
fi

if [ "$(git -C "${CHECKOUT_DIR}" rev-parse HEAD)" != "${SUBMODULE_COMMIT}" ]; then
    echo "${CHECKOUT_DIR} does not match the duckdb-wasm submodule; remove it and rerun" >&2
    exit 1
fi

SUBMODULE_COMMON_DIR="$(git -C "${SUBMODULE_DIR}" rev-parse --path-format=absolute --git-common-dir)"
CHECKOUT_COMMON_DIR="$(git -C "${CHECKOUT_DIR}" rev-parse --path-format=absolute --git-common-dir)"
if [ "${CHECKOUT_COMMON_DIR}" != "${SUBMODULE_COMMON_DIR}" ]; then
    echo "${CHECKOUT_DIR} was not created from the duckdb-wasm submodule; remove it and rerun" >&2
    exit 1
fi

PATCH_MARKER="${CHECKOUT_DIR}/.superhuman-docs-patched"
if [ ! -f "${PATCH_MARKER}" ]; then
    make -C "${CHECKOUT_DIR}" apply_patches
    git -C "${CHECKOUT_DIR}" apply "${PATCH_FILE}"
    touch "${PATCH_MARKER}"
fi

yarn --cwd "${CHECKOUT_DIR}" install --frozen-lockfile
for variant in mvp eh coi; do
    SUPERHUMAN_DOCS_EXTENSION_DIR="${EXTENSION_DIR}" \
        DUCKDB_WASM_VERSION="superhuman-docs" \
        "${CHECKOUT_DIR}/scripts/wasm_build_lib.sh" relperf "${variant}"
done
yarn --cwd "${CHECKOUT_DIR}" workspace @duckdb/duckdb-wasm build:release

echo "Built-in bundle sizes:"
for variant in mvp eh coi; do
    bundle="${CHECKOUT_DIR}/packages/duckdb-wasm/dist/duckdb-${variant}.wasm"
    echo "duckdb-${variant}.wasm $(wc -c < "${bundle}") bytes"
done
