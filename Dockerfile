FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# Install base dependencies + Docker CLI
RUN apt-get update && apt-get install -y \
    curl \
    git \
    sudo \
    xz-utils \
    ca-certificates \
    docker.io \
    && rm -rf /var/lib/apt/lists/*

# Install Nix (Determinate Systems installer — no init system in containers)
RUN curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | \
    sh -s -- install linux --no-confirm --init none

ENV PATH="/nix/var/nix/profiles/default/bin:${PATH}"

# Configure Nix: trust the flake, and set up cachix substituters
RUN mkdir -p /root/.config/nix && \
    cat > /root/.config/nix/nix.conf << 'EOF'
accept-flake-config = true
extra-substituters = https://xmtp.cachix.org https://cache.garnix.io
extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0= cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g=
EOF

# Install direnv + nix-direnv so the Nix devShell environment is available
# to all processes (terminals, language servers, etc.) via .envrc.
# nix-daemon is not running during build, so start it inline for this step.
RUN nix-daemon & sleep 1 && \
    nix profile install nixpkgs#direnv nixpkgs#nix-direnv && \
    kill %1 2>/dev/null || true

# Configure direnv: hook into bash, use nix-direnv for cached evaluation
RUN mkdir -p /root/.config/direnv && \
    cat > /root/.config/direnv/direnvrc << 'DIRENVRC'
source $HOME/.nix-profile/share/nix-direnv/direnvrc
DIRENVRC

# Trust mounted repos, hook nix-daemon + direnv into bash
RUN git config --global --add safe.directory '*' && \
    cat >> /root/.bashrc << 'BASHRC'
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
if ! pgrep -x nix-daemon > /dev/null; then nix-daemon & disown; fi
eval "$(direnv hook bash)"
BASHRC

WORKDIR /workspaces/libxmtp

LABEL org.opencontainers.image.description="libxmtp Nix Development Container"
LABEL org.opencontainers.image.url="https://github.com/xmtp/libxmtp"
