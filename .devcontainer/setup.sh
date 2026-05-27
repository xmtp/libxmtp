#!/usr/bin/env bash
set -euo pipefail

# Mark workspace as safe for git (bind-mount has different ownership metadata)
git config --global safe.directory /workspaces/libxmtp

# When @devcontainers/cli starts the container with the default
# updateRemoteUserUID: true, it remaps `vscode` to match the host's UID but
# only chowns $HOME — leaving /nix (baked to UID 1000 at image build) owned
# by an orphaned UID. Detect the mismatch and repair /nix so single-user nix
# works for the current user. No-op when UIDs already match (the common case
# on macOS and on Linux hosts with UID 1000).
nix_owner="$(stat -c %u /nix)"
vscode_uid="$(id -u)"
if [[ "$nix_owner" != "$vscode_uid" ]]; then
    sudo chown -R "$vscode_uid:$(id -g)" /nix
fi

# Allow direnv before any shell session starts
direnv allow
