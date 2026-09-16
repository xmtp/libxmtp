"""Run Codex shell commands in this worktree's Nix environment."""

import json
from pathlib import Path
import shlex
import sys


def rewrite(payload, root):
    """Keep command text and execution permissions unchanged."""
    tool_input = payload.get("tool_input", {})
    command = tool_input.get("command")
    if payload.get("tool_name") != "Bash" or not isinstance(command, str):
        return {}
    cwd = Path(tool_input.get("cwd") or payload.get("cwd") or Path.cwd()).resolve()
    if not cwd.is_relative_to(root):
        return {}
    updated = dict(tool_input)
    updated["command"] = "exec " + shlex.join(
        [
            str(root / "dev/nix-shell"),
            "--command",
            "bash",
            "-c",
            'export XMTP_RTK="${XMTP_RTK-1}"; ' + command,
        ]
    )
    return {
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            # Required by Codex for a rewrite, not a PermissionRequest approval.
            "permissionDecision": "allow",
            "updatedInput": updated,
        }
    }


if __name__ == "__main__":
    print(
        json.dumps(rewrite(json.load(sys.stdin), Path(__file__).resolve().parents[2]))
    )
