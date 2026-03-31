#!/usr/bin/env bash
set -euo pipefail

# Ensure nix profile bins are on PATH for this script
export PATH="$HOME/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"

# Fix nix store ownership (feature installs as root, vscode needs write access)
sudo chown -R vscode:vscode /nix

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
nix-env -iA nixpkgs.direnv

# Configure nix-direnv integration
mkdir -p ~/.config/direnv
echo 'source ~/.nix-profile/share/nix-direnv/direnvrc' > ~/.config/direnv/direnvrc

# Source nix profile and fix stack limit in .zshenv (loaded before .zshrc)
if ! grep -q 'nix.sh' ~/.zshenv 2>/dev/null; then
  cat >> ~/.zshenv << 'NIXEOF'

# Raise stack size for Nix
ulimit -s unlimited 2>/dev/null

# Nix
if [ -e '/nix/var/nix/profiles/default/etc/profile.d/nix.sh' ]; then
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
