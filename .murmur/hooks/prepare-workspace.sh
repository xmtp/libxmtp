#!/bin/bash
# Murmur workspace hook. Runs on an agent VM after the repos are cloned and
# immediately before the agent starts, as the agent user, with the agent's
# environment. It never runs for a local developer, so nothing here may change
# how this repo behaves outside a Murmur VM.
#
# All workspace hooks share a 5-minute budget, and a non-zero exit fails the
# spawn before the agent starts. Keep this fast and keep it total: anything
# that can fail on a transient condition must not abort the spawn.
set -euo pipefail

# --- Ref MCP server -----------------------------------------------------
# Ref is a remote HTTP MCP server. Claude Code wants the key as a header in
# ~/.claude.json; Codex wants it in ~/.codex/config.toml. Neither reads
# $REF_API_KEY on its own, and the key is a personal credential, so it cannot
# be baked into the shared image. It arrives at runtime from
# `murmur secret mount REF_API_KEY`.
#
# This writes the agent's own config on the VM rather than a file in the repo,
# which is what keeps local developers unaffected: a developer who already has
# Ref configured, or who does not use it at all, sees no change from this repo.
REF_URL="https://api.plan.ref.tools/mcp"

if [ -z "${REF_API_KEY:-}" ]; then
  echo "prepare-workspace: REF_API_KEY not mounted; skipping Ref MCP setup"
else
  # Claude Code: merge into ~/.claude.json, preserving every other key.
  if ! python3 - "$HOME/.claude.json" "$REF_URL" <<'PY'
import json, os, sys

path, url = sys.argv[1], sys.argv[2]
key = os.environ["REF_API_KEY"]

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
  then
    # A missing Ref server makes the agent less capable, not broken. Never
    # fail the spawn over it.
    echo "prepare-workspace: WARNING: could not configure Ref for Claude Code" >&2
  fi
  chmod 0600 "$HOME/.claude.json" 2>/dev/null || true

  # Codex: merge into ~/.codex/config.toml. Codex resolves the key from the
  # environment through env_http_headers, so the file holds the variable name
  # rather than the secret.
  mkdir -p "$HOME/.codex"
  touch "$HOME/.codex/config.toml"
  if ! python3 - "$HOME/.codex/config.toml" "$REF_URL" <<'PY'
import os, re, sys

path, url = sys.argv[1], sys.argv[2]

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
text += (
    '\n[mcp_servers.ref-plan]\n'
    f'url = "{url}"\n'
    'env_http_headers = { "x-ref-api-key" = "REF_API_KEY" }\n'
)

tmp = path + ".tmp"
with open(tmp, "w") as fh:
    fh.write(text)
os.replace(tmp, path)
PY
  then
    echo "prepare-workspace: WARNING: could not configure Ref for Codex" >&2
  fi
  chmod 0600 "$HOME/.codex/config.toml" 2>/dev/null || true
fi

echo "prepare-workspace: done"
