"""Keep early compiler diagnostics visible after noisy parallel Xcode output."""

from collections import deque
from pathlib import Path
import re
import sys


ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
DIAGNOSTIC = re.compile(
    r"\berror:|fatal error|\b[A-Za-z]+Error:|"
    r"(?:macro|plug-?in).*(?:could not|failed|not found|not permitted|denied)|"
    r"operation not permitted|permission denied|unable to spawn|"
    r"no such file|killed(?::| by| process)|terminated by signal",
    re.IGNORECASE,
)


def summarize(path: Path) -> list[str]:
    selected = deque(maxlen=60)
    with path.open(errors="replace") as source:
        for number, line in enumerate(source, 1):
            text = ANSI.sub("", line.rstrip())
            if DIAGNOSTIC.search(text):
                selected.append(f"{number}: {text[:1000]}")
    return list(selected)


if __name__ == "__main__":
    print(
        "RN iOS consumer failure diagnostics (last 60 matches, 1000 characters each):"
    )
    rows = summarize(Path(sys.argv[1]))
    print("\n".join(rows) if rows else "No matching compiler diagnostic was emitted.")
