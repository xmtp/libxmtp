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
# Determinate Nix owns /etc/nix/nix.conf and documents nix.custom.conf as the
# only supported place for extra settings, so write there and leave its file
# alone. Overwriting nix.conf loses whatever the installer put in it.
#
# `trusted-users` is the setting that matters most here. This repo's flake
# declares nixConfig { http-connections, max-substitution-jobs, sandbox }, and
# `accept-flake-config` makes the client forward them to the daemon. All three
# are restricted settings: the daemon discards them, with a warning on every
# single nix invocation, unless the requesting user is trusted. `sandbox =
# relaxed` is not cosmetic -- `nix build .#nextest` needs it.
cat > /etc/nix/nix.custom.conf <<'EOF'
experimental-features = nix-command flakes
accept-flake-config = true
trusted-users = root murmur
sandbox = relaxed
max-jobs = auto
extra-substituters = https://xmtp.cachix.org
extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=
trusted-substituters = https://xmtp.cachix.org
# The warm store is one large substitution; the defaults (16 and 25) leave the
# instance's bandwidth idle. Agents also re-fetch after flake.lock moves.
max-substitution-jobs = 32
http-connections = 64
warn-dirty = false
EOF
chmod 0644 /etc/nix/nix.custom.conf

# Pick up the settings above, then prove they actually took effect as the
# user that will use them. Both of these have already been silently wrong on
# a shipped image: trusted-users showed up only as a warning on every nix
# call, and the missing cache only as builds that were slower than expected.
systemctl restart nix-daemon.service || true
sleep 2
nix_effective="$(sudo -u murmur -H bash -lc 'nix config show' 2>/dev/null || true)"
if echo "$nix_effective" | grep -E '^trusted-users' | grep -qw murmur; then
  echo "nix: trusted-users includes murmur"
else
  echo "WARNING: murmur is not a trusted nix user. The flake's nixConfig" >&2
  echo "         (sandbox, http-connections, max-substitution-jobs) will be" >&2
  echo "         discarded on every nix invocation, and nix build .#nextest" >&2
  echo "         needs sandbox = relaxed." >&2
fi
if echo "$nix_effective" | grep -q 'xmtp.cachix.org'; then
  echo "nix: xmtp.cachix.org is an effective substituter"
else
  echo "WARNING: xmtp.cachix.org is not in the effective substituters; every" >&2
  echo "         agent build will miss the cache that CI populates." >&2
fi

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

echo "=== Warm the Nix store and the Docker images ==="
# Every cost paid here is a cost no agent VM pays again. Three things are warm
# after this stage: the `default` and `rust` dev shell closures, the backend
# musl container image, and the upstream Docker images `just backend up` needs.
#
# This clones the repo ONLY to evaluate the flake. Nix copies what it needs
# into /nix/store, which is what the snapshot keeps; the checkout itself is
# deleted before the image is captured. No customer source ends up in the
# image -- agents still clone fresh at spawn.
#
# The warm store is pinned to flake.lock and Cargo.lock as of bake time. When
# those move, agents re-fetch whatever changed. Rebake periodically to keep
# the image warm. See .murmur/nightly-rebake.md.
WARM_DIR=/tmp/libxmtp-warm
GCROOTS=/nix/var/nix/gcroots/libxmtp-warm
GH_STACK_VERSION=v0.1.1
rm -rf "$WARM_DIR"
# The warm steps build as `murmur`, and creating the --out-link symlink needs
# write permission on this directory. Root's `mkdir` leaves it 0755 root-owned,
# which would fail every warm step.
mkdir -p "$GCROOTS"
chown murmur:murmur "$GCROOTS"

# The recipe timeout is 1h and that is the platform maximum, so this stage
# must never consume the whole budget. Every step reports its own elapsed
# time, because a bake that runs long is otherwise impossible to diagnose
# after the fact -- the platform keeps no per-step timing.
WARM_START=$(date +%s)
warm_elapsed() { echo $(( ($(date +%s) - WARM_START) / 60 )); }
stage() { echo "=== [$(warm_elapsed)m] $* ==="; }

# Seconds a step may take: whichever is smaller, the caller's own cap in
# minutes or what is left before WARM_DEADLINE_MIN. Every capped step goes
# through this, so no combination of steps can overrun the recipe timeout.
# Negative means there is no budget left and the step must be skipped.
WARM_DEADLINE_MIN=50
warm_remaining() {
  local want_min="$1" left
  left=$(( WARM_START + WARM_DEADLINE_MIN * 60 - $(date +%s) ))
  [ "$left" -lt 0 ] && left=0
  if [ "$left" -gt $(( want_min * 60 )) ]; then echo $(( want_min * 60 )); else echo "$left"; fi
}

warm_failed=0

# Run a warm step as the murmur user and report how long it took.
# Usage: warm_step <label> <timeout> <command...>
warm_step() {
  local label="$1" limit="$2"
  shift 2
  local began rc
  began=$(date +%s)
  stage "start: $label"
  # Declare rc before the call: `local` is itself a command and would reset $?.
  rc=0
  timeout "$limit" sudo -u murmur -H bash -lc "cd $WARM_DIR && $*" || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "=== [$(warm_elapsed)m] done: $label ($(( ($(date +%s) - began) / 60 ))m) ==="
    return 0
  fi
  if [ "$rc" -eq 124 ]; then
    echo "NOTE: $label hit its ${limit} cap; agents will build it on first use" >&2
  else
    echo "WARNING: $label failed (exit $rc)" >&2
  fi
  return "$rc"
}

if git clone --depth 1 --branch self-hosted \
     https://github.com/xmtp/libxmtp "$WARM_DIR" 2>&1; then
  chown -R murmur:murmur "$WARM_DIR"

  # Build the devShell derivations rather than entering them. A devShell's
  # output records its build inputs as runtime dependencies, so a single
  # out-link under gcroots pins the entire closure -- no requisites walk, and
  # no cap on how much gets pinned.
  #
  # The `rust` shell is what .envrc selects and what backend.just defaults to;
  # `default` is what `dev/nix-shell` picks with no NIX_DEVSHELL set.
  # Cap each shell against the budget that is actually left, not a fixed 20m.
  # Two fixed 20m caps plus the later steps' caps total more than the 1h
  # recipe ceiling, so a slow pair of shells could run the bake out of time
  # before the guards below ever apply. `warm_remaining` keeps every step
  # inside one shared deadline.
  for shell in default rust; do
    limit=$(warm_remaining 20)
    if [ "$limit" -le 0 ]; then
      stage "SKIPPING $shell dev shell warm: no budget left"
      warm_failed=1
      continue
    fi
    warm_step "$shell dev shell" "${limit}s" \
      "nix build '.#devShells.x86_64-linux.$shell' --out-link '$GCROOTS/shell-$shell'" \
      || warm_failed=1
  done

  # GitHub CLI extensions live in the user's home directory, not in the Nix
  # store. Install gh-stack as murmur so every agent can use it through the
  # repo shell. Pin the release so an image rebuild cannot change the CLI.
  gh_stack_limit=$(warm_remaining 5)
  if [ "$gh_stack_limit" -le 0 ]; then
    echo "ERROR: no time remains to install gh-stack" >&2
    exit 1
  fi
  if ! warm_step "gh-stack $GH_STACK_VERSION" "${gh_stack_limit}s" \
    "./dev/nix-shell 'gh extension install github/gh-stack --pin $GH_STACK_VERSION --force' && ./dev/nix-shell 'gh stack --version'"; then
    echo "ERROR: gh-stack is part of the agent toolchain and must be installed" >&2
    exit 1
  fi

  # `just backend up` depends on `just backend image`, which cross-compiles the
  # backend for musl. This is the single most expensive cold-start cost.
  # fh-cache.yml pushes this image to Cachix on every push to self-hosted, so
  # the normal case is a download; the cap covers the cache-miss case, which
  # happens when a commit changes the closure and nothing matches the cache.
  #
  # Build it outside any devshell, per the note in backend.just: a devshell
  # puts a newer glibc on the library path and breaks the git that Nix uses
  # to fetch git dependencies.
  backend_limit=$(warm_remaining 15)
  # Needs a useful slice of time to be worth starting; a 2-minute stub would
  # just burn budget and time out anyway.
  if [ "$backend_limit" -ge 300 ]; then
    warm_step "backend musl image" "${backend_limit}s" \
      "nix build .#backend-image-x86_64-unknown-linux-musl --out-link '$GCROOTS/backend-image'" \
      || true  # Not a bake failure: the image is still valid without this.
  else
    stage "SKIPPING backend image warm: $(warm_elapsed)m elapsed, not enough budget"
    echo "    Budget reserved for finishing the bake. Agents will build the" >&2
    echo "    backend image on their first \`just backend up\`." >&2
  fi

  # Pre-load every image the stack starts. Without this, each VM pulls the
  # whole set on its first `just backend up`. /var/lib/docker is part of the
  # snapshot, so what lands here ships in the image.
  stage "start: docker images"
  systemctl start docker.service || true
  if [ -e "$GCROOTS/backend-image" ]; then
    docker load --input "$GCROOTS/backend-image" || \
      echo "WARNING: could not load the backend image into docker" >&2
  fi
  # Let compose name the images: it reads dev/docker/compose.yml, so this
  # cannot drift from the stack the agents actually start. Compose pulls them
  # in parallel, which matters because the daemon's max-concurrent-downloads
  # applies per pull and serial pulls leave the instance's bandwidth idle.
  #
  # Every variable in compose.yml has a default, so this parses without the
  # per-worktree env file that dev/docker/up generates.
  #
  # --policy missing skips the backend image loaded just above, which is built
  # by Nix and does not exist in any registry. --ignore-pull-failures keeps a
  # transient registry failure from aborting the bake; a VM that pulls one
  # image on first use is a small cost.
  #
  # One timeout covers the whole group. Serial pulls with a timeout each could
  # add 30 minutes on top of the shell and backend steps, run the bake past
  # the 1h platform ceiling, and produce no image at all.
  IMAGE_BUDGET=$(warm_remaining 10)
  if [ "$IMAGE_BUDGET" -le 0 ]; then
    echo "NOTE: out of budget; skipping the image pulls" >&2
  else
    # Both stacks an agent can start: the main one, and the separate TLS stack
    # behind `just backend tls-check`. Passing both files to one `pull` keeps
    # them under the single budget above.
    timeout "${IMAGE_BUDGET}s" docker compose \
      -f "$WARM_DIR/dev/docker/compose.yml" \
      -f "$WARM_DIR/dev/tls/compose.yml" \
      pull --policy missing --ignore-pull-failures --quiet \
      || echo "WARNING: could not pre-pull every compose image" >&2
  fi
  stage "done: docker images"
else
  echo "WARNING: could not clone libxmtp to warm the store" >&2
  warm_failed=1
fi

# Remove the checkout. Only /nix/store paths and /var/lib/docker survive.
rm -rf "$WARM_DIR"

stage "warming stage complete"

if [ "$warm_failed" -ne 0 ]; then
  # A warm miss makes agents slower, not broken -- they rebuild on demand.
  # Do not fail the bake over it.
  echo "NOTE: one or more warm steps failed; agents will build those on first use" >&2
fi

echo "=== Install the just shim ==="
# Agents reach for bare `just check`. Nothing is on the bare PATH on this
# image, so without a shim that fails and the agent has to remember the
# `dev/nix-shell '...'` wrapper on every single call.
#
# dev/nix-shell short-circuits when it is already inside a matching
# environment, so a recipe that shells out to `just` again does not enter Nix
# twice. Outside a libxmtp checkout this falls through to the store's `just`.
cat > /usr/local/bin/just <<'SHIM_EOF'
#!/usr/bin/env bash
# Run `just` inside this worktree's Nix environment. See .murmur/persona.md.
set -euo pipefail
if root="$(git rev-parse --show-toplevel 2>/dev/null)" \
   && [ -x "$root/dev/nix-shell" ]; then
  exec "$root/dev/nix-shell" --command just "$@"
fi
exec nix run nixpkgs#just -- "$@"
SHIM_EOF
chmod 0755 /usr/local/bin/just

echo "=== Verify the murmur user can reach the toolchain ==="
# This is the contract that matters: the bake runs as root, agents run as
# murmur. Prove nix is on murmur's PATH and functional before snapshotting.
sudo -u murmur -H bash -lc 'nix --version'
sudo -u murmur -H bash -lc 'docker --version'
sudo -u murmur -H bash -lc 'git --version'
# The warm closures must still be in the store, and reachable by their roots.
for shell in default rust; do
  test -e "/nix/var/nix/gcroots/libxmtp-warm/shell-$shell" \
    || echo "WARNING: $shell dev shell was not warmed" >&2
done

echo "=== Clean up ==="
apt-get clean
rm -rf /var/lib/apt/lists/*
# No nix-collect-garbage here. Every warm closure is rooted under
# /nix/var/nix/gcroots/libxmtp-warm, so a collection would only reclaim
# build-time inputs, and it pays a full store scan to do it. The disk is
# 200 GB and the image is captured once.

echo "=== Provisioning complete ==="
