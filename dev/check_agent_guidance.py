"""Check literal repository references and Just commands in tracked agent guidance."""

import json
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
# Runtime output, not source references.
GENERATED = ("target/", "node_modules/", "dev/docker/.env")
LINK = re.compile(r"\[[^\]\n]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
CODE = re.compile(r"(?<!`)`([^`\n]+)`(?!`)")
JUST = re.compile(r"\bjust\s+([a-zA-Z_][\w:-]*)(?:[ \t]+([a-zA-Z_][\w-]*))?")


def exists(root, doc, path):
    return (doc.parent / path).exists() or (root / path).exists()


def literal_path(token, root, doc):
    """Only whole path tokens; omit globs, shell fragments, and placeholders."""
    if (
        not re.fullmatch(r"[\w./-]+", token)
        or "/" not in token
        or token.startswith("/")
    ):
        return False
    # `./gradlew` names a command, not a source file location.
    if token.startswith("./") and "/" not in token[2:] and not Path(token).suffix:
        return False
    first = token.split("/", 1)[0]
    return (
        first in {".", ".."} or (root / first).is_dir() or (doc.parent / first).is_dir()
    )


def command_error(first, second, tree):
    parts = first.split("::")
    if len(parts) == 1 and first in tree["modules"]:
        if not second:
            return None  # Running a module's default recipe is valid.
        parts.append(second)
    node = tree
    for module in parts[:-1]:
        if module not in node["modules"]:
            return f"unknown Just module: {module}"
        node = node["modules"][module]
    recipe = parts[-1]
    if recipe not in node["recipes"] and recipe not in node.get("aliases", {}):
        return f"unknown Just recipe: {' '.join(parts)}"
    # Root check/test dispatch to private recipes rather than Just modules.
    if len(parts) == 1 and recipe in {"check", "test"} and second:
        if f"_{recipe}-{second}" not in node["recipes"]:
            return f"unknown Just target: {recipe} {second}"
    return None


def check_document(root, doc, tree):
    failures = []
    for number, line in enumerate(doc.read_text().splitlines(), 1):
        prefix = f"{doc.relative_to(root)}:{number}"
        for match in LINK.finditer(line):
            target = urlsplit(match[1])
            if target.scheme or target.netloc or not target.path:
                continue
            path = unquote(target.path)
            if not exists(root, doc, path):
                failures.append(f"{prefix}: missing link target: {path}")
        for match in CODE.finditer(line):
            token = match[1].rstrip("/")
            if (
                literal_path(token, root, doc)
                and not token.startswith(GENERATED)
                and not exists(root, doc, token)
            ):
                failures.append(f"{prefix}: missing repository path: {token}")
        for match in JUST.finditer(line):
            error = command_error(*match.groups(), tree)
            if error:
                failures.append(f"{prefix}: {error}")
    return failures


def main():
    paths = subprocess.check_output(
        ["git", "ls-files", "-z"], cwd=ROOT, text=True
    ).split("\0")
    tree = json.loads(
        subprocess.check_output(
            ["just", "--dump", "--dump-format", "json"], cwd=ROOT, text=True
        )
    )
    failures = []
    for path in paths:
        if Path(path).name == "AGENTS.md" or (
            path.startswith(".agents/skills/") and path.endswith("/SKILL.md")
        ):
            doc = ROOT / path
            if doc.is_file():
                failures.extend(check_document(ROOT, doc, tree))
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print("Agent guidance references and Just recipes are valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
