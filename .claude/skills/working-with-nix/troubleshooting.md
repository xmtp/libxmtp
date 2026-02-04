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

There is no workaround. The iOS shell requires:
- Darwin-specific toolchains
- Xcode build tools
- Apple's cctools

For iOS development, you must use macOS. Consider:
- Using a macOS CI runner
- Remote development on a Mac
- For testing only: use the simulator on a Mac

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

If you need to use system Rust for other projects, consider:
- Using direnv to auto-switch
- Running `nix develop` explicitly for this project
- Using `nix develop --command cargo build` for one-off commands

**Do not** modify the Rust version in `flake.nix` without project coordination.

---

## Issue: direnv Not Loading

**Symptoms:**
- Entering the directory doesn't activate Nix
- `which cargo` shows system cargo, not Nix
- No shell prompt change

**Causes & Solutions:**

1. **direnv not installed:**
   ```bash
   # Run the setup script
   ./dev/nix-up
   ```

2. **direnv not allowed for this directory:**
   ```bash
   direnv allow
   ```

3. **Shell hook not configured:**
   ```bash
   # Add to ~/.bashrc or ~/.zshrc
   eval "$(direnv hook bash)"  # or zsh
   ```

4. **direnv cache stale:**
   ```bash
   direnv reload
   ```

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

The default shell has WASM tooling but the `.#wasm` shell is specifically configured for WASM builds with:
- Correct environment variables
- WASM-specific Rust toolchain
- Pre-configured build targets

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
- `$EMULATOR` command fails
- "ANDROID_HOME not set"
- AVD manager errors

**Cause:** Not in Android shell or virtualization issues.

**Solution:**

1. Use the Android shell:
   ```bash
   nix develop .#android
   ```

2. Verify environment:
   ```bash
   echo $ANDROID_HOME
   echo $NDK_HOME
   ```

3. Check virtualization (Linux):
   ```bash
   # KVM must be available
   ls /dev/kvm
   ```

4. On macOS, ensure virtualization extensions are enabled.

---

## Debugging Commands

### Verbose Nix Output

```bash
# Show full trace on errors
nix develop --show-trace

# Very verbose
nix develop -vvv
```

### Interactive Exploration

```bash
# REPL for exploring the flake
nix repl .

# In REPL:
:lf .                           # Load flake
devShells.aarch64-darwin.default  # Inspect shell
```

### Check Flake Health

```bash
# Show all outputs
nix flake show

# Check for issues
nix flake check

# View input versions
nix flake metadata
```

### Garbage Collection

```bash
# Remove old generations (free disk space)
nix-collect-garbage -d

# Remove specific age
nix-collect-garbage --delete-older-than 7d
```

---

## Getting Help

1. **Check Nix documentation:** https://nixos.org/manual/nix/stable/
2. **Flake-parts docs:** https://flake.parts/
3. **Fenix (Rust):** https://github.com/nix-community/fenix
4. **Ask in team chat** with your error output and `nix flake metadata` results
