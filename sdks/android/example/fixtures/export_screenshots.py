#!/usr/bin/env python3
"""Export only named instrumentation fixture images, then remove owned files."""

import argparse
from pathlib import Path
import subprocess

APP = "org.xmtp.android.example"
PRIVATE = "files/xmtp-messenger-proof"
PNG = b"\x89PNG\r\n\x1a\n"
LIMIT = 10 * 1024 * 1024
SCREENS = {
    "start",
    "conversations",
    "create",
    "timeline",
    "conversation_settings",
    "group_fields",
    "my_fields",
    "app_settings",
    "drafts",
}
NAMES = {
    "setup.png",
    "conversations.png",
    "create.png",
    "create-group.png",
    "messages.png",
    "settings.png",
    "app-settings.png",
    "ux-start-no-auth.png",
    "ux-start-auth.png",
    "ux-inline-reactions.png",
    "ux-full-reactions.png",
    "ux-reply-composer.png",
    "ux-inline-reactions-200.png",
    "ux-full-reactions-200.png",
    "attachment-card-verified.png",
    "group-settings-pending-remove.png",
} | {
    f"scale-{prefix}{screen}.png"
    for prefix in ("", "unreachable-", "clipped-")
    for screen in SCREENS
}


def owned(serial, *arguments):
    return subprocess.run(
        ["adb", "-s", serial, "exec-out", "run-as", APP, *arguments],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=30,
    ).stdout


def export(serial, output):
    failure = None
    count = 0
    try:
        output.mkdir(parents=True, exist_ok=True)
        for name in NAMES:
            (output / name).unlink(missing_ok=True)
        listing = owned(
            serial, "sh", "-c", f"if test -d {PRIVATE}; then ls {PRIVATE}; fi"
        )
        names = listing.decode("utf-8").splitlines()
        if len(names) != len(set(names)) or not set(names) <= NAMES:
            raise ValueError("Unknown or duplicate fixture screenshot name")
        for name in names:
            image = owned(serial, "head", "-c", str(LIMIT + 1), f"{PRIVATE}/{name}")
            if not image.startswith(PNG) or len(image) > LIMIT:
                raise ValueError("Invalid fixture PNG")
            (output / name).write_bytes(image)
            count += 1
    except BaseException as error:
        failure = error
    try:
        owned(serial, "rm", "-rf", PRIVATE)
    except BaseException as error:
        if failure is None:
            failure = error
        else:
            failure.add_note(
                f"Owned screenshot cleanup also failed: {type(error).__name__}"
            )
    if failure is not None:
        raise failure
    return count


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    print(f"Exported {export(args.serial, args.output)} fixture screenshots")
