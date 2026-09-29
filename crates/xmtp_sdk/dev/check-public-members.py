#!/usr/bin/env python3
"""Check that installed public products keep every retained Client member.

The expected members come from the SDK API manifest, which the inventory and
its destination rules build from the old SDK sources. They do not come from
the generator, so a member that generation omits fails here even when the raw
binding still has it. The actual members come from the staged public
products that `just sdk public-consumer` installs: public Swift and Kotlin
declarations of the host Client, and the TypeScript compiler's view of the
installed Node and browser package roots.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "docs/self-hosted/sdk-api-manifest.md"
STAGE = ROOT / "target/sdk-public"
RETAINED = {"generated", "static runtime"}
# A destination whose reference ends in `open` is an open item, not a decided
# member of the new SDK. The manifest lists it under "Open items".
UNDECIDED = re.compile(r"(^|; )open$")


Member = tuple[str, str]  # (placement, name); placement is "static" or "instance"


def host_name(sdk: str, placement: str, destination: str) -> str:
    """Map a manifest destination to the member name on a host Client.

    `inboxId(for:)` is Swift's static `inboxId(for:backend:)`; every other
    form of it is `inboxIdFor`.
    """
    base = destination.split("(")[0]
    if destination.endswith("(for:)") and not (
        sdk == "Swift" and placement == "static"
    ):
        return base + "For"
    return base


def expected() -> dict[str, set[Member]]:
    """Map each SDK section to its retained Client members and placements."""
    members: dict[str, set[Member]] = {}
    section = None
    for line in MANIFEST.read_text().splitlines():
        heading = re.match(r"^## (Swift|Kotlin|Node|Browser)$", line)
        if heading:
            section = heading.group(1)
            members[section] = set()
            continue
        if line.startswith("## "):
            section = None
        if section is None or not line.startswith("| `"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        final, status, reference = cells[2], cells[3], cells[4]
        match = re.match(r"^`(static )?Client\.([A-Za-z_]+(?:\(for:\))?)", final)
        if match and status in RETAINED and not UNDECIDED.search(reference):
            placement = "static" if match.group(1) else "instance"
            members[section].add(
                (placement, host_name(section, placement, match.group(2)))
            )
    return members


def blocks(source: str, header: re.Pattern[str]) -> list[str]:
    """Return the body of each brace block whose opening line matches."""
    bodies = []
    for match in header.finditer(source):
        start = source.index("{", match.end() - 1)
        depth = 0
        for index in range(start, len(source)):
            depth += {"{": 1, "}": -1}.get(source[index], 0)
            if depth == 0:
                bodies.append(source[start + 1 : index])
                break
    return bodies


def top_level(body: str) -> list[str]:
    """Return the lines of a block body at its own brace depth."""
    lines, depth = [], 0
    for line in body.splitlines():
        if depth == 0:
            lines.append(line)
        depth += line.count("{") - line.count("}")
    return lines


def swift_members() -> set[Member]:
    source = "\n".join(
        path.read_text()
        for path in sorted((STAGE / "XmtpSdk/Sources/XmtpSdk").rglob("*.swift"))
    )
    members = set()
    member = re.compile(
        r"^\s*(?:(public)\s+)?(?:(static|class)\s+)?(?:func|var|let)\s+`?(\w+)"
    )

    def add(found: re.Match[str]) -> None:
        members.add(("static" if found.group(2) else "instance", found.group(3)))

    for body in blocks(
        source, re.compile(r"^public final class SDKClient\b[^{]*\{", re.M)
    ):
        for line in top_level(body):
            found = member.match(line)
            if found and found.group(1):
                add(found)
    for body in blocks(source, re.compile(r"^public extension SDKClient\s*\{", re.M)):
        for line in top_level(body):
            found = member.match(line)
            if found and not re.match(r"^\s*(private|fileprivate|internal)\b", line):
                add(found)
    return members


def kotlin_members() -> set[Member]:
    source = "\n".join(
        path.read_text() for path in sorted((STAGE / "kotlin").rglob("*.kt"))
    )
    members = set()
    hidden = re.compile(r"^\s*(private|internal|protected)\b")
    member = re.compile(r"^\s*(?:override\s+)?(?:suspend\s+)?(?:fun|val|var)\s+`?(\w+)")
    for body in blocks(source, re.compile(r"^class SDKClient\b[^{]*\{", re.M)):
        for line in top_level(body):
            found = member.match(line)
            if found and not hidden.match(line):
                members.add(("instance", found.group(1)))
        for companion in blocks(body, re.compile(r"^\s*companion object\s*\{", re.M)):
            for line in top_level(companion):
                found = member.match(line)
                if found and not hidden.match(line):
                    members.add(("static", found.group(1)))
    extension = re.compile(r"^(?:suspend\s+)?fun\s+SDKClient\.`?(\w+)", re.M)
    members.update(("instance", name) for name in extension.findall(source))
    return members


def typescript_missing(
    consumer: str, package: str, members: set[Member], lib: list[str]
) -> set[Member]:
    """Return the members that are not public on the package root Client.

    `keyof` leaves out private and protected members, so the compiler rejects
    a member that is missing, not public, or in the other placement.
    """
    probe = STAGE / consumer / "retained-members.ts"
    ordered = sorted(members)
    lines = [
        f'import {{ Client }} from "{package}";',
        "type Instance<K extends PropertyKey> = K extends keyof Client ? true : never;",
        "type Static<K extends PropertyKey> = K extends keyof typeof Client ? true : never;",
    ]
    first = len(lines) + 1
    lines += [
        f'export const m{index}: {"Static" if placement == "static" else "Instance"}<"{name}"> = true;'
        for index, (placement, name) in enumerate(ordered)
    ]
    probe.write_text("\n".join(lines) + "\n")
    result = subprocess.run(
        [
            str(ROOT / "node_modules/.bin/tsc"),
            "--noEmit",
            "--strict",
            "--skipLibCheck",
            "--target",
            "es2022",
            "--module",
            "esnext",
            "--moduleResolution",
            "bundler",
            "--allowImportingTsExtensions",
            *lib,
            str(probe),
        ],
        capture_output=True,
        text=True,
    )
    missing = set()
    for line in result.stdout.splitlines():
        found = re.match(r"^.*retained-members\.ts\((\d+),\d+\): error", line)
        if not found:
            if "error TS" in line:
                raise SystemExit(f"{consumer}: {line}")
            continue
        index = int(found.group(1)) - first
        if not 0 <= index < len(ordered):
            raise SystemExit(f"{consumer}: {line}")
        missing.add(ordered[index])
    if result.returncode != 0 and not missing:
        raise SystemExit(f"{consumer}: compile failed\n{result.stdout}{result.stderr}")
    return missing


def main() -> None:
    wanted = expected()
    missing = {
        "Swift": wanted["Swift"] - swift_members(),
        "Kotlin": wanted["Kotlin"] - kotlin_members(),
        "Node": typescript_missing("node-consumer", "xmtp-sdk", wanted["Node"], []),
        "Browser": typescript_missing(
            "browser-consumer",
            "xmtp-sdk-browser",
            wanted["Browser"],
            ["--lib", "es2022,dom,dom.iterable"],
        ),
    }
    failures = []
    for sdk, names in wanted.items():
        if not names:
            failures.append(f"{sdk}: the manifest lists no retained Client member")
        for placement, name in sorted(missing[sdk]):
            failures.append(
                f"{sdk}: retained {placement} Client.{name} is not public in the installed product"
            )
    if failures:
        print("\n".join(failures), file=sys.stderr)
        sys.exit(1)
    counts = ", ".join(f"{sdk} {len(names)}" for sdk, names in wanted.items())
    print(f"Installed public Clients keep every retained member ({counts})")


if __name__ == "__main__":
    main()
