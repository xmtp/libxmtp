#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

installer=.cache/capy/nix-installer
rm -rf "$installer"
if [[ ! -x "$HOME/.nix-profile/bin/nix" ]] ||
  [[ "$("$HOME/.nix-profile/bin/nix" --version)" != 'nix (Nix) 2.31.5' ]]; then
  mkdir -p "$installer"
  curl --fail --location --retry 3 \
    https://releases.nixos.org/nix/nix-2.31.5/nix-2.31.5-x86_64-linux.tar.xz \
    -o "$installer/nix.tar.xz"
  printf '%s  %s\n' \
    fa2d848b0e155b144b6e5e312f04306b2136fa26ead168778909bc13dd9da4d9 \
    "$installer/nix.tar.xz" | sha256sum --check
  tar -xJf "$installer/nix.tar.xz" -C "$installer"
  bash "$installer/nix-2.31.5-x86_64-linux/install" \
    --no-daemon --yes --no-channel-add --no-modify-profile
  rm -rf "$installer"
fi

mkdir -p "$HOME/.local/bin"
for binary in "$HOME/.nix-profile/bin"/nix*; do
  ln -sfn "$(readlink -f "$binary")" "$HOME/.local/bin/$(basename "$binary")"
done

nix_conf="${XDG_CONFIG_HOME:-$HOME/.config}/nix/nix.conf"
mkdir -p "$(dirname "$nix_conf")"
touch "$nix_conf"
for setting in \
  'extra-substituters = https://xmtp.cachix.org' \
  'extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0='; do
  if ! grep -qxF "$setting" "$nix_conf"; then
    printf '%s\n' "$setting" >> "$nix_conf"
  fi
done
