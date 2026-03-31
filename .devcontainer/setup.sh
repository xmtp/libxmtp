#!/usr/bin/env bash
# Log all output for debugging (Coder's envbuilder swallows stdout/stderr)
exec > >(tee -a /tmp/devcontainer-setup.log) 2>&1
set -euo pipefail

# Source the nix environment if it exists (feature may have installed it)
for nix_sh in \
  "$HOME/.nix-profile/etc/profile.d/nix.sh" \
  "/nix/var/nix/profiles/default/etc/profile.d/nix.sh" \
  "/etc/profile.d/nix.sh"; do
  if [ -e "$nix_sh" ]; then
    # shellcheck disable=SC1090
    . "$nix_sh"
    break
  fi
done
export PATH="$HOME/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"

# If nix still isn't available, the devcontainer feature wasn't applied
# (e.g. Coder envbuilder skips features). Install nix as a fallback.
if ! command -v nix-env &>/dev/null; then
  echo "Nix not found — installing (devcontainer feature was not applied)..."
  curl -fsSL https://nixos.org/nix/install | bash -s -- --no-daemon --no-modify-profile
  # shellcheck disable=SC1091
  . "$HOME/.nix-profile/etc/profile.d/nix.sh"

  # Write the nix config that the feature would have provided
  mkdir -p ~/.config/nix
  cat > ~/.config/nix/nix.conf << 'NIXCONF'
experimental-features = nix-command flakes
accept-flake-config = true
build-users-group =
extra-substituters = https://xmtp.cachix.org https://cache.garnix.io
extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ca1F+Tmq0= cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g=
NIXCONF
fi

# Fix nix store ownership (feature installs as root, vscode needs write access)
if [ -d /nix ] && [ "$(stat -c '%U' /nix 2>/dev/null)" != "$(whoami)" ]; then
  sudo chown -R "$(whoami):$(id -gn)" /nix
fi

# Raise stack size hard limit for Nix (needs 60MB+, default is often 10MB)
echo '* hard stack unlimited' | sudo tee -a /etc/security/limits.conf > /dev/null
echo '* soft stack unlimited' | sudo tee -a /etc/security/limits.conf > /dev/null

# Mark workspace as safe for git (bind-mount has different ownership metadata)
git config --global --add safe.directory /workspaces/libxmtp

# Install Docker CLI + Compose if not already provided by the environment
if ! command -v docker &> /dev/null; then
  sudo apt-get update && sudo apt-get install -y docker.io docker-compose-v2 && sudo rm -rf /var/lib/apt/lists/*
fi

# Install direnv via nix-env (nix-direnv is already installed by the feature)
nix-env -iA nixpkgs.direnv nixpkgs.nix-direnv

# Configure nix-direnv integration
mkdir -p ~/.config/direnv
echo 'source ~/.nix-profile/share/nix-direnv/direnvrc' > ~/.config/direnv/direnvrc

# Source nix profile and fix stack limit in .zshenv (loaded before .zshrc)
if ! grep -q 'nix.sh' ~/.zshenv 2>/dev/null; then
  cat >> ~/.zshenv << 'NIXEOF'

# Raise stack size for Nix
ulimit -s unlimited 2>/dev/null

# Nix
if [ -e "$HOME/.nix-profile/etc/profile.d/nix.sh" ]; then
  . "$HOME/.nix-profile/etc/profile.d/nix.sh"
elif [ -e '/nix/var/nix/profiles/default/etc/profile.d/nix.sh' ]; then
  . '/nix/var/nix/profiles/default/etc/profile.d/nix.sh'
fi
NIXEOF
fi

# Hook direnv into zsh (after nix PATH is set via .zshenv)
if ! grep -q 'direnv hook zsh' ~/.zshrc 2>/dev/null; then
  echo 'eval "$(direnv hook zsh)"' >> ~/.zshrc
fi

# Trust the workspace
direnv allow /workspaces/libxmtp
