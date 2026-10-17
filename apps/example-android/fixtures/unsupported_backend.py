"""Run one app test command with an owned backend that has no attachment storage.

It runs the current Nix backend binary on an owned database of this worktree's
stack, with dev/backend/local.toml, and passes the backend to the command as
XMTP_ANDROID_UNSUPPORTED_BACKEND_URL and XMTP_ANDROID_UNSUPPORTED_BACKEND_PORT.
"""

from metadata_backend import UNSUPPORTED, main

if __name__ == "__main__":
    main(UNSUPPORTED, __doc__)
