#!/usr/bin/env bash
# Redirect ALL output (stdout, stderr, and xtrace) to a persistent log file
# so we can debug failures even when the devcontainer runner swallows output.
LOG="/tmp/devcontainer-setup.log"
exec > >(tee -a "$LOG") 2>&1
set -euxo pipefail

echo "=== setup.sh started at $(date -u) ==="
echo "PWD=$(pwd) USER=$(whoami) HOME=$HOME"
echo "PATH=$PATH"
echo "/nix exists: $(ls -d /nix 2>&1 || echo 'NO')"
echo "nix-profile: $(ls -la "$HOME/.nix-profile" 2>&1 || echo 'MISSING')"
echo "which sudo: $(command -v sudo 2>&1 || echo 'MISSING')"
echo "which nix-env: $(command -v nix-env 2>&1 || echo 'MISSING')"

# Source the full nix environment (PATH, NIX_PROFILES, NIX_SSL_CERT_FILE, etc.)
# The nix devcontainer feature may install as root; we need to find the profile
# script from whichever user context it was installed for.
for nix_sh in \
  "$HOME/.nix-profile/etc/profile.d/nix.sh" \
  "/nix/var/nix/profiles/default/etc/profile.d/nix.sh" \
  "/etc/profile.d/nix.sh"; do
  if [ -e "$nix_sh" ]; then
    echo "Sourcing $nix_sh"
    # shellcheck disable=SC1090
    . "$nix_sh"
    break
  fi
done

# Fallback: ensure nix profile bins are on PATH even if no profile script found
export PATH="$HOME/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
echo "PATH after nix setup: $PATH"
echo "which nix-env (after): $(command -v nix-env 2>&1 || echo 'STILL MISSING')"

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

echo "=== setup.sh completed successfully ==="
