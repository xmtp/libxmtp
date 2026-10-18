export NIX_DEVSHELL := env("NIX_DEVSHELL", "js")
set shell := ["../../dev/nix-shell"]
root := justfile_directory()
_env := ". " + root + "/dev/docker/load-env"

# Build JS/types without realizing the full native product closure.
check:
    cd "{{ root }}" && dev/js/sdk-package react-native
    cd "{{ root }}" && pnpm --filter @xmtp/react-native-sdk run typecheck

# Realize and export the declared immutable components on their native host.
components platform out-dir:
    cd "{{ root }}" && python3.11 sdks/react-native/scripts/products.py components {{ quote(platform) }} {{ quote(out-dir) }}

# Import both producer closures, then build and export the canonical Linux product.
package android-dir ios-dir:
    cd "{{ root }}" && python3.11 sdks/react-native/scripts/products.py package {{ quote(android-dir) }} {{ quote(ios-dir) }}

# Load the exact assembled tarball closure before installed consumer tests.
load-package export-dir:
    cd "{{ root }}" && python3.11 sdks/react-native/scripts/products.py load-package {{ quote(export-dir) }}

# Installed Hermes bare/Expo proofs; consumers never build Rust.
test-installed platform selection="all":
    {{ _env }} && cd "{{ root }}" && dev/nix-shell --shell react-native --command sdks/react-native/scripts/test-installed {{ quote(platform) }} {{ quote(selection) }}

test-conformance platform:
    {{ _env }} && cd "{{ root }}" && dev/nix-shell --shell react-native --command sdks/react-native/scripts/test-installed {{ quote(platform) }} all --conformance

# Compile the production lifecycle/ownership helpers against a controlled ABI.
test-native:
    cd "{{ root }}" && dev/nix-shell --shell react-native --command python3.11 sdks/react-native/test/native/run.py

# React ownership tests use the real sibling native public classes and errors.
test-react:
    cd "{{ root }}" && dev/js/sdk-package node
    cd "{{ root }}" && pnpm --filter @xmtp/react-native-sdk run test

# Real HTTP reload settlement and final-result admission; no SDK build required.
test-control:
    cd "{{ root }}" && node --test sdks/react-native/test/conformance/peer-runtime.test.mjs

# Provenance and binary completeness checks; these use no emulator or backend.
check-package:
    cd "{{ root }}" && python3.11 -B -m unittest discover -s sdks/react-native/scripts -p 'test_*.py'

# Evaluate cache identities with the repository's actual Nix source boundaries.
check-cache:
    cd "{{ root }}" && python3.11 -B crates/xmtp_sdk/dev/check-generated-nix.py
