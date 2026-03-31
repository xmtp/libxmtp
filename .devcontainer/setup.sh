#!/usr/bin/env bash
set -euo pipefail

# Mark workspace as safe for git (bind-mount has different ownership metadata)
git config --global safe.directory /workspaces/libxmtp
