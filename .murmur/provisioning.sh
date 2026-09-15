#!/bin/bash
set -euo pipefail

# Provisioning for the libxmtp workspace image.
#
# libxmtp gets its entire toolchain from a Nix flake: Rust 1.97.1, Node 24,
# JDK, just, foundry, sqlcipher, and the rest. This script therefore installs
# Nix itself (multi-user daemon mode), the xmtp Cachix substituter, and Docker
# for the backend test services. It does NOT install Rust or Node from apt --
# the flake owns those, and an apt copy would only shadow it.

export DEBIAN_FRONTEND=noninteractive

echo "=== apt: base build dependencies ==="
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates \
  xz-utils \
  build-essential \
  pkg-config \
  libssl-dev \
  postgresql-client \
  sudo

echo "=== Install Nix (multi-user daemon mode) ==="
# The Determinate Systems installer is non-interactive and configures the
# daemon, /etc/nix/nix.conf, and the systemd unit in one step.
curl --proto '=https' --tlsv1.2 -sSf -L \
  https://install.determinate.systems/nix \
  | sh -s -- install linux --init systemd --no-confirm

# Make the daemon profile visible to every login shell, including the
# non-interactive `bash -c` shells that agents use.
NIX_PROFILE_SH=/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
if [ ! -f "$NIX_PROFILE_SH" ]; then
  echo "FATAL: nix daemon profile not found at $NIX_PROFILE_SH" >&2
  exit 1
fi
cat > /etc/profile.d/10-nix.sh <<'EOF'
if [ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]; then
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
fi
EOF
chmod 0644 /etc/profile.d/10-nix.sh

# Non-login, non-interactive shells read neither /etc/profile nor ~/.bashrc,
# so also put the nix binaries on the default PATH via symlinks.
for bin in nix nix-build nix-shell nix-store nix-env nix-collect-garbage; do
  if [ -e "/nix/var/nix/profiles/default/bin/$bin" ]; then
    ln -sf "/nix/var/nix/profiles/default/bin/$bin" "/usr/local/bin/$bin"
  fi
done

echo "=== Configure Nix: flakes + xmtp Cachix substituter ==="
mkdir -p /etc/nix
# Mirrors .devcontainer/nix.conf so agent builds hit the same binary cache CI
# uses. trusted-users lets the murmur user set flake config without sudo.
cat > /etc/nix/nix.conf <<'EOF'
experimental-features = nix-command flakes
accept-flake-config = true
build-users-group = nixbld
trusted-users = root murmur
sandbox = relaxed
max-jobs = auto
substituters = https://cache.nixos.org https://xmtp.cachix.org
trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=
trusted-substituters = https://xmtp.cachix.org
netrc-file = /etc/nix/netrc
warn-dirty = false
EOF
chmod 0644 /etc/nix/nix.conf

# The Determinate installer writes its own nix.conf include; restart so the
# daemon picks up the substituter list above.
systemctl restart nix-daemon.service || true

echo "=== Install Docker Engine + compose plugin ==="
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
  -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
ARCH="$(dpkg --print-architecture)"
CODENAME="$(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")"
cat > /etc/apt/sources.list.d/docker.list <<EOF
deb [arch=${ARCH} signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu ${CODENAME} stable
EOF
apt-get update
apt-get install -y --no-install-recommends \
  docker-ce \
  docker-ce-cli \
  containerd.io \
  docker-buildx-plugin \
  docker-compose-plugin

# Agents run as `murmur` and need the docker socket for `just backend up`.
usermod -aG docker murmur
systemctl enable docker.service || true
systemctl enable containerd.service || true

echo "=== Prepare the murmur user's nix directories ==="
# `nix develop` writes into ~/.cache/nix; create these up front with the right
# owner so the first agent build does not trip over a root-owned directory.
install -d -o murmur -g murmur -m 0755 /home/murmur/.cache
install -d -o murmur -g murmur -m 0755 /home/murmur/.cache/nix
install -d -o murmur -g murmur -m 0755 /home/murmur/.config
install -d -o murmur -g murmur -m 0755 /home/murmur/.config/nix
cat > /home/murmur/.config/nix/nix.conf <<'EOF'
experimental-features = nix-command flakes
accept-flake-config = true
EOF
chown murmur:murmur /home/murmur/.config/nix/nix.conf
chmod 0644 /home/murmur/.config/nix/nix.conf

echo "=== Warm the Nix store: dev shell + backend image ==="
# Without this, every fresh VM pays a multi-minute download for the dev shell
# closure, and the first `just backend up` pays a full musl cross-compile of
# the backend. Both costs belong here, paid once per image.
#
# This clones the repo ONLY to evaluate the flake. Nix copies what it needs
# into /nix/store, which is what the snapshot keeps; the checkout itself is
# deleted before the image is captured. No customer source ends up in the
# image -- agents still clone fresh at spawn.
#
# The warm store is pinned to flake.lock and Cargo.lock as of bake time. When
# those move, agents re-fetch whatever changed. Rebake periodically to keep
# the image warm.
WARM_DIR=/tmp/libxmtp-warm
rm -rf "$WARM_DIR"

warm_failed=0
if git clone --depth 1 --branch self-hosted \
     https://github.com/xmtp/libxmtp "$WARM_DIR" 2>&1; then
  chown -R murmur:murmur "$WARM_DIR"

  # `nix develop --command true` realizes the full devShell closure without
  # entering an interactive shell. Run as murmur so the per-user profile and
  # eval cache are warmed for the account that actually builds.
  echo "--- warming the default dev shell ---"
  if sudo -u murmur -H bash -lc "cd $WARM_DIR && nix develop .#default --command true"; then
    echo "default dev shell warmed"
  else
    echo "WARNING: default dev shell warm failed" >&2
    warm_failed=1
  fi

  # The `rust` shell is what .envrc selects and what backend.just defaults to.
  echo "--- warming the rust dev shell ---"
  if sudo -u murmur -H bash -lc "cd $WARM_DIR && nix develop .#rust --command true"; then
    echo "rust dev shell warmed"
  else
    echo "WARNING: rust dev shell warm failed" >&2
    warm_failed=1
  fi

  # `just backend up` depends on `just backend image`, which cross-compiles
  # the backend for musl. This is the single most expensive cold-start cost.
  # Build it outside any devshell, per the note in backend.just: a devshell
  # puts a newer glibc on the library path and breaks the git that Nix uses
  # to fetch git dependencies.
  echo "--- warming the backend musl container image ---"
  if sudo -u murmur -H bash -lc \
       "cd $WARM_DIR && nix build .#backend-image-x86_64-unknown-linux-musl --no-link"; then
    echo "backend image warmed"
  else
    echo "WARNING: backend image warm failed" >&2
    warm_failed=1
  fi

  # Pin every warmed closure with a GC root so `nix-collect-garbage` below
  # cannot delete the very paths this stage just paid for.
  echo "--- pinning warmed closures against GC ---"
  mkdir -p /nix/var/nix/gcroots/libxmtp-warm
  sudo -u murmur -H bash -lc \
    "cd $WARM_DIR && nix build .#backend-image-x86_64-unknown-linux-musl \
       --out-link /tmp/warm-backend-image" || true
  if [ -e /tmp/warm-backend-image ]; then
    cp -P /tmp/warm-backend-image /nix/var/nix/gcroots/libxmtp-warm/backend-image
  fi
  # A devShell's build inputs are not the output of `nix build` on the shell
  # derivation, so root the .drv itself: that keeps the inputs `nix develop`
  # pulled in from being collected.
  for shell in default rust; do
    drv="$(sudo -u murmur -H bash -lc \
      "cd $WARM_DIR && nix path-info --derivation .#devShells.x86_64-linux.$shell" \
      2>/dev/null || true)"
    if [ -n "$drv" ] && [ -e "$drv" ]; then
      ln -sfn "$drv" "/nix/var/nix/gcroots/libxmtp-warm/shell-$shell.drv"
      # Root the realized inputs too, not just the recipe for building them.
      sudo -u murmur -H bash -lc \
        "nix-store --query --requisites '$drv'" 2>/dev/null \
        | head -4000 > "/tmp/warm-reqs-$shell" || true
    fi
  done

  # Keep every realized path the warm steps produced. Indexed symlinks under
  # a gcroots directory are the documented way to pin arbitrary store paths.
  idx=0
  for reqfile in /tmp/warm-reqs-default /tmp/warm-reqs-rust; do
    [ -f "$reqfile" ] || continue
    while IFS= read -r storepath; do
      [ -n "$storepath" ] || continue
      [ -e "$storepath" ] || continue
      idx=$((idx + 1))
      ln -sfn "$storepath" \
        "/nix/var/nix/gcroots/libxmtp-warm/req-$idx" 2>/dev/null || true
    done < "$reqfile"
  done
  echo "pinned $idx warmed store paths against GC"
  rm -f /tmp/warm-reqs-default /tmp/warm-reqs-rust
else
  echo "WARNING: could not clone libxmtp to warm the store" >&2
  warm_failed=1
fi

# Remove the checkout. Only /nix/store paths survive into the image.
rm -rf "$WARM_DIR" /tmp/warm-backend-image /tmp/warm-shell-default /tmp/warm-shell-rust

if [ "$warm_failed" -ne 0 ]; then
  # A warm miss makes agents slower, not broken -- they rebuild on demand.
  # Do not fail the bake over it.
  echo "NOTE: one or more warm steps failed; agents will build those on first use" >&2
fi

echo "=== Install the Ref MCP bootstrap ==="
# Ref is a remote HTTP MCP server. Both Claude Code and Codex want the API key
# written INSIDE a config file -- Claude as an x-ref-api-key header in
# ~/.claude.json, Codex as a query parameter in ~/.codex/config.toml. Neither
# reads $REF_API_KEY on its own.
#
# The key is a PERSONAL credential, so it must never be baked into this shared
# image. Instead the image ships this generator, and the key arrives at runtime
# from `murmur secret mount REF_API_KEY`. The script is a no-op when the
# variable is absent, so an agent without the mount simply runs without Ref.
cat > /usr/local/bin/murmur-ref-mcp-setup <<'SETUP_EOF'
#!/usr/bin/env bash
# Wire the Ref MCP server into Claude Code and Codex using $REF_API_KEY.
# Safe to run repeatedly; it rewrites only the Ref entries.
set -uo pipefail

if [ -z "${REF_API_KEY:-}" ]; then
  exit 0
fi

REF_URL="https://api.plan.ref.tools/mcp"

# --- Claude Code: merge into ~/.claude.json, preserving everything else ---
CLAUDE_CFG="$HOME/.claude.json"
python3 - "$CLAUDE_CFG" "$REF_URL" <<'PY'
import json, os, sys

path, url = sys.argv[1], sys.argv[2]
key = os.environ.get("REF_API_KEY", "")
if not key:
    sys.exit(0)

try:
    with open(path) as fh:
        cfg = json.load(fh)
    if not isinstance(cfg, dict):
        cfg = {}
except (FileNotFoundError, ValueError):
    cfg = {}

servers = cfg.get("mcpServers")
if not isinstance(servers, dict):
    servers = {}
servers["ref-plan"] = {
    "type": "http",
    "url": url,
    "headers": {"x-ref-api-key": key},
}
cfg["mcpServers"] = servers

tmp = path + ".tmp"
with open(tmp, "w") as fh:
    json.dump(cfg, fh, indent=2)
os.replace(tmp, path)
PY
chmod 0600 "$CLAUDE_CFG" 2>/dev/null || true

# --- Codex: merge into ~/.codex/config.toml ---
# Codex takes the key as a query parameter rather than a header.
CODEX_DIR="$HOME/.codex"
CODEX_CFG="$CODEX_DIR/config.toml"
mkdir -p "$CODEX_DIR"
touch "$CODEX_CFG"
python3 - "$CODEX_CFG" "$REF_URL" <<'PY'
import os, re, sys
from urllib.parse import quote

path, base_url = sys.argv[1], sys.argv[2]
key = os.environ.get("REF_API_KEY", "")
if not key:
    sys.exit(0)

with open(path) as fh:
    text = fh.read()

# Drop any existing ref-plan table so repeated runs do not stack duplicates.
text = re.sub(
    r"^\[mcp_servers\.ref-plan\]\s*\n(?:(?!^\[).*\n?)*",
    "",
    text,
    flags=re.MULTILINE,
)

if text and not text.endswith("\n"):
    text += "\n"
url = base_url + "?apiKey=" + quote(key, safe="")
text += '\n[mcp_servers.ref-plan]\nurl = "' + url + '"\n'

tmp = path + ".tmp"
with open(tmp, "w") as fh:
    fh.write(text)
os.replace(tmp, path)
PY
chmod 0600 "$CODEX_CFG" 2>/dev/null || true
SETUP_EOF
chmod 0755 /usr/local/bin/murmur-ref-mcp-setup

# Run it on every login shell so the config exists before an agent starts.
# It re-reads $REF_API_KEY each time, so a rotated key takes effect on the
# next VM with no rebake.
cat > /etc/profile.d/20-ref-mcp.sh <<'EOF'
# Wire up the Ref MCP server when a personal REF_API_KEY is mounted.
if [ -n "${REF_API_KEY:-}" ] && [ -x /usr/local/bin/murmur-ref-mcp-setup ]; then
  /usr/local/bin/murmur-ref-mcp-setup >/dev/null 2>&1 || true
fi
EOF
chmod 0644 /etc/profile.d/20-ref-mcp.sh

echo "=== Verify the murmur user can reach the toolchain ==="
# This is the contract that matters: the bake runs as root, agents run as
# murmur. Prove nix is on murmur's PATH and functional before snapshotting.
sudo -u murmur -H bash -lc 'nix --version'
sudo -u murmur -H bash -lc 'docker --version'
sudo -u murmur -H bash -lc 'git --version'

echo "=== Clean up ==="
apt-get clean
rm -rf /var/lib/apt/lists/*
# Only delete paths with no GC root. The warm closures above are rooted
# under /nix/var/nix/gcroots/libxmtp-warm, so they survive this.
nix-collect-garbage >/dev/null 2>&1 || true

echo "=== Provisioning complete ==="
