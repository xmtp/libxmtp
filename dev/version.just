# SDK versions are generated from workspace.package.version in Cargo.toml.
set working-directory := ".."

check:
    python3.11 -B dev/sdk_version.py check

set version:
    python3.11 -B dev/sdk_version.py set {{ quote(version) }}
