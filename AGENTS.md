# Repository instructions

## Formatting

- Always run changed source files through the repository formatter before considering work complete.
- Use the locally installed Python to create a temporary virtual environment for formatting tools; do not add formatter dependencies to the project.
- Install the same formatter versions used by DuckDB CI: `cmake-format`, `black==24.*`, `cxxheaderparser`, `pcpp`, and `clang_format==11.0.1`.
- Run `make format-fix`, then verify with `make format-check`. The local `duckdb` and `extension-ci-tools` submodules contain the formatting rules and scripts.
