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
for tool in curl xz git shasum; do
  if ! command -v "$tool" >/dev/null; then
    "${run_root[@]}" apt-get update
    "${run_root[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y \
      ca-certificates curl xz-utils git perl
    break
  fi
done

# Also find an installation from an earlier build without relying on shell rc files.
export PATH="$HOME/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
if ! command -v nix >/dev/null; then
  installer="$(mktemp)"
  trap 'rm -f "$installer"' EXIT
  curl --proto '=https' --tlsv1.2 --fail --show-error --location \
    https://nixos.org/nix/install --output "$installer"
  # Single-user Nix needs no daemon or systemd after restoring the snapshot.
  # An empty build-users-group also permits a root-owned single-user store.
  NIX_CONFIG="${NIX_CONFIG:-}
build-users-group =" sh "$installer" \
    --no-daemon --yes --no-channel-add --no-modify-profile
  rm -f "$installer"
  trap - EXIT
fi

config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/nix"
mkdir -p "$config_dir"
cat > "$config_dir/libxmtp-droid.conf" <<'EOF_CONFIG'
experimental-features = nix-command flakes
extra-substituters = https://xmtp.cachix.org
extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=
EOF_CONFIG
if [[ ! -f "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh" ]]; then
  # The outer cloud environment supplies isolation for the single-user store.
  printf '%s\n' 'sandbox = false' 'build-users-group =' >> "$config_dir/libxmtp-droid.conf"
fi
include_line="include $config_dir/libxmtp-droid.conf"
touch "$config_dir/nix.conf"
if ! grep -qxF "$include_line" "$config_dir/nix.conf"; then
  printf '\n%s\n' "$include_line" >> "$config_dir/nix.conf"
fi

# Persist PATH for terminals; dev/nix-shell also locates Nix in non-login shells.
for profile in "$HOME/.profile" "$HOME/.bashrc"; do
  touch "$profile"
  if ! grep -qF '# libxmtp Droid Nix' "$profile"; then
    temporary="$(mktemp)"
    {
      # shellcheck disable=SC2016 # Expand PATH when the later shell starts.
      printf '%s\n' '# libxmtp Droid Nix' \
        'export PATH="$HOME/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"'
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
