# Nix Troubleshooting Guide

Common issues and solutions for libxmtp Nix development.

## Issue: First Build is Extremely Slow

**Symptoms:**
- `nix develop` takes 20+ minutes
- Seeing lots of "building" or "fetching" messages
- CPU usage is high for extended periods

**Cause:** Cachix binary cache not being used.

**Solution:**
1. Verify the cache is configured:
   ```bash
   nix config show | grep substituters
   # Should include: https://xmtp.cachix.org
   ```

2. If missing, the flake's `nixConfig` should add it automatically. Check that your Nix installation trusts flake configs:
   ```bash
   # In /etc/nix/nix.conf or ~/.config/nix/nix.conf
   # Should have:
   accept-flake-config = true
   ```

3. Manually add if needed:
   ```bash
   # Add to nix.conf
   extra-substituters = https://xmtp.cachix.org
   extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=
   ```

---

## Issue: New File Not Found During Build

**Symptoms:**
- `nix build` or `nix develop` says a file doesn't exist
- File exists on disk and is visible with `ls`
- Build worked before adding the new file

**Cause:** Nix flakes only see git-tracked files.

**Solution:**
```bash
git add <new-file>
# Then retry the nix command
```

This is a fundamental property of Nix flakes for reproducibility. The flake's source is determined by git, not the filesystem.

---

## Issue: iOS Shell Fails on Linux

**Symptoms:**
- `nix develop .#ios` immediately fails
- Error mentions missing `darwin` or `xcbuild`
- Works on a colleague's Mac

**Cause:** The iOS shell is macOS-only.

**Solution:**

There is no workaround. The iOS shell requires Darwin-specific toolchains and Xcode. For iOS development, you must use macOS.

---

## Issue: Xcode Version Too Old for iOS

**Symptoms:**
- Warning on shell entry: "Xcode X.X detected. Xcode 16+ required for Swift 6.1"
- Swift Package Manager fails with Package Traits errors

**Cause:** Xcode < 16 doesn't support Swift 6.1 Package Traits.

**Solution:**
```bash
# Check current version
xcodebuild -version

# Install Xcode 16+ from App Store, then:
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

---

## Issue: iOS Build Uses Wrong Compiler

**Symptoms:**
- iOS cross-compilation fails with `-mmacos-version-min` errors
- Linker errors mentioning macOS flags during iOS builds
- Nix's cc-wrapper injecting wrong flags

**Cause:** Nix's stdenv overrides `DEVELOPER_DIR` to its own apple-sdk. The `/usr/bin/clang` shim reads this and dispatches to Nix's cc-wrapper, which injects macOS-specific flags that break iOS compilation.

**Solution:**

The iOS shell (`ios.nix`) and default shell (`local.nix`) handle this automatically by:
1. Resolving the real Xcode path via `/usr/bin/xcode-select`
2. Setting `CC_aarch64_apple_ios` (etc.) to the full Xcode toolchain clang path
3. Unsetting `SDKROOT` so xcrun discovers per-target SDKs

If you encounter this outside the Nix shells, ensure you're using `nix develop .#ios` or `nix develop`.

---

## Issue: Rust Version Mismatch

**Symptoms:**
- `rustc --version` shows wrong version outside Nix
- Build fails with "requires Rust 1.92.0"
- Cargo.toml's rust-version check fails

**Cause:** System Rust differs from project's pinned version.

**Solution:**

Always use the Nix shell for development:
```bash
nix develop
# Now rustc --version shows 1.92.0
```

**Do not** modify the Rust version in `flake.nix` without project coordination.

---

## Issue: direnv Not Loading

**Symptoms:**
- Entering the directory doesn't activate Nix
- `which cargo` shows system cargo, not Nix
- No shell prompt change

**Causes & Solutions:**

1. **direnv not installed:** Run `./dev/nix-up`
2. **direnv not allowed:** Run `direnv allow`
3. **Shell hook not configured:** Add `eval "$(direnv hook bash)"` (or zsh) to shell rc
4. **Cache stale:** Run `direnv reload`

---

## Issue: WASM Build Fails

**Symptoms:**
- `wasm-pack build` fails
- Missing `wasm32-unknown-unknown` target
- Linker errors mentioning WASM

**Cause:** Wrong shell or missing WASM tooling.

**Solution:**

Use the dedicated WASM shell:
```bash
nix develop .#wasm
wasm-pack build --target web bindings/wasm
```

The WASM shell uses Chrome/ChromeDriver for testing (not Firefox). It has a separate `fenix.stable` Rust toolchain with the WASM target pre-configured.

---

## Issue: OpenSSL Errors

**Symptoms:**
- "Can't locate openssl headers"
- "openssl-sys" crate build fails
- Linking errors with libssl

**Cause:** OpenSSL environment not set correctly.

**Solution:**

1. Ensure you're in a Nix shell:
   ```bash
   nix develop
   echo $OPENSSL_DIR  # Should be set
   ```

2. If building outside Nix (not recommended), set:
   ```bash
   export OPENSSL_DIR=$(brew --prefix openssl)  # macOS
   export OPENSSL_DIR=/usr  # Linux
   ```

3. Force vendored OpenSSL (last resort):
   ```bash
   unset OPENSSL_NO_VENDOR
   cargo build  # Will build OpenSSL from source
   ```

---

## Issue: Android Emulator Won't Start

**Symptoms:**
- `run-test-emulator` fails or hangs
- "ANDROID_HOME not set"
- Port binding errors

**Cause:** Not in Android shell, virtualization issues, or port conflicts.

**Solution:**

1. Use the Android shell:
   ```bash
   nix develop .#android
   ```

2. Launch the emulator:
   ```bash
   run-test-emulator  # Custom script, scans ports 5560-5584
   ```

3. The custom `run-test-emulator` script avoids ports 5554-5558 which conflict with Docker services started by `dev/docker/up`. If the emulator still hangs, check:
   ```bash
   # Ensure Docker services aren't using ports in 5560+ range
   lsof -i :5560-5584
   ```

4. Check virtualization (Linux): `ls /dev/kvm`

---

## Issue: Node Build Fails with Sandbox Error

**Symptoms:**
- `nix build .#node-bindings-js` fails with network errors
- Sandbox violation during `yarn install`

**Cause:** The `node-bindings-js` package uses `__noChroot = true` because it needs network access for `yarn install`. Linux enforces `sandbox=true` by default.

**Solution:**

Run `node-bindings-js` builds on macOS, which doesn't enforce the Nix sandbox. For Linux, you would need to set `sandbox = false` in `nix.conf` (not recommended for general use).

The per-target `.node` builds (`node-bindings-*`) do NOT require network access and work on both platforms.

---

## Debugging Commands

### Verbose Nix Output

```bash
nix develop --show-trace   # Show full trace on errors
nix develop -vvv           # Very verbose
```

### Interactive Exploration

```bash
nix repl .
# In REPL:
:lf .                           # Load flake
devShells.aarch64-darwin.default  # Inspect shell
```

### Check Flake Health

```bash
nix flake show      # Show all outputs
nix flake check     # Check for issues
nix flake metadata  # View input versions
```

### Garbage Collection

```bash
nix-collect-garbage -d              # Remove old generations
nix-collect-garbage --delete-older-than 7d  # Remove specific age
```
