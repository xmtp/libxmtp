#!/usr/bin/env bash
set -euo pipefail

# Mark workspace as safe for git (bind-mount has different ownership metadata)
git config --global safe.directory /workspaces/libxmtp

# Allow direnv before any shell session starts
direnv allow
