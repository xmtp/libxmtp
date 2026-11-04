#!/usr/bin/env bash
# Factory Environment repository setup: run before the snapshot is captured.
set -euo pipefail

if [[ $# -ne 0 || $(uname -s) != Linux ]]; then
  echo 'Run without arguments inside a disposable Linux Factory Environment.' >&2
  exit 1
fi
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_root=()
if [[ $EUID -ne 0 ]]; then
  run_root=(sudo -n)
fi
for tool in curl xz git shasum flock start-stop-daemon; do
  if ! command -v "$tool" >/dev/null; then
    "${run_root[@]}" apt-get update
    "${run_root[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y \
      ca-certificates curl xz-utils git perl util-linux dpkg
    break
  fi
done

# Also find an installation from an earlier build without relying on shell rc files.
export PATH="/nix/var/nix/profiles/default/bin:$PATH"
if [[ -x "/nix/var/nix/profiles/default/bin/nix-daemon" && ! -f "/etc/nix/libxmtp-droid.conf" ]]; then
  echo 'Unmanaged Nix installation found. Rebuild from a fresh Linux source.' >&2
  exit 1
fi
if [[ ! -x "/nix/var/nix/profiles/default/bin/nix-daemon" ]]; then
  if command -v nix >/dev/null; then
    echo 'Existing single-user Nix is unsupported. Rebuild from a fresh Linux source.' >&2
    exit 1
  fi
  installer="$(mktemp)"
  trap 'rm -f "$installer"' EXIT
  curl --proto '=https' --tlsv1.2 --fail --show-error --location \
    https://install.determinate.systems/nix/tag/v3.24.1 --output "$installer"
  # Install build users and a root-owned store, but manage daemon startup ourselves
  # so snapshot restoration does not depend on systemd.
  "${run_root[@]}" sh "$installer" install linux --no-confirm --init none
  rm -f "$installer"
  trap - EXIT
fi

config="$(mktemp)"
trap 'rm -f "$config"' EXIT
cat > "$config" <<'EOF_CONFIG'
experimental-features = nix-command flakes
sandbox = true
sandbox-fallback = false
extra-substituters = https://xmtp.cachix.org
extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=
EOF_CONFIG
"${run_root[@]}" install -o root -g root -m 0644 "$config" "/etc/nix/libxmtp-droid.conf"
rm -f "$config"
trap - EXIT
# Determinate manages nix.conf and reads nix.custom.conf.
nix_conf="/etc/nix/nix.conf"
[[ ! -f "/etc/nix/nix.custom.conf" ]] || nix_conf="/etc/nix/nix.custom.conf"
include_line='include /etc/nix/libxmtp-droid.conf'
if ! grep -qxF "$include_line" "$nix_conf"; then
  printf '\n%s\n' "$include_line" | "${run_root[@]}" tee -a "$nix_conf" >/dev/null
fi
"${run_root[@]}" install -o root -g root -m 0755 \
  "$repo_root/.factory/nix-daemon.sh" "/usr/local/sbin/libxmtp-nix-daemon"
"${run_root[@]}" "/usr/local/sbin/libxmtp-nix-daemon"
export NIX_REMOTE=daemon

# Persist PATH for terminals; dev/nix-shell also locates Nix in non-login shells.
for profile in "$HOME/.profile" "$HOME/.bashrc"; do
  touch "$profile"
  if ! grep -qF '# libxmtp Droid Nix' "$profile"; then
    temporary="$(mktemp)"
    {
      # shellcheck disable=SC2016 # Expand PATH when the later shell starts.
      printf '%s\n' '# libxmtp Droid Nix' \
        'export PATH="/nix/var/nix/profiles/default/bin:$PATH"'
      cat "$profile"
    } > "$temporary"
    cat "$temporary" > "$profile"
    rm -f "$temporary"
  fi
done

nix --version
# Realize the closures, not just flake metadata. The wrapper creates persistent
# GC-rooted profiles and caches print-dev-env for the snapshot's checkout.
for shell in default rust js; do
  echo "Preloading Nix shell: $shell"
  dev/nix-shell --shell "$shell" 'true'
done
dev/nix-shell 'rustc --version; cargo --version; node --version; pnpm --version; just --version'
# Check the default workspace members with the locked dependencies. Keep Cargo's
# downloads, target directory, and compiler cache in the snapshot for reuse.
echo 'Warming Cargo dependencies and build caches with a locked cargo check'
dev/nix-shell 'just check'
echo 'Nix shell closures and Cargo caches are ready for snapshotting.'
