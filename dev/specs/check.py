#!/usr/bin/env python3
"""Validate specs/ and the requirement backlinks in the repository.

Three commands, all reached through just recipes:

    just spec-check            validate; exit 1 on an error
    just spec-index [--json]   print every requirement with its links
    just spec-show ID          print one requirement with its links

The checker parses the specs itself rather than reading generated state, so
nothing has to be committed and regenerated. See specs/SPEC-spec-format.md for
the rules it enforces; every check names the requirement it comes from.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

# SPEC-030: three to five uppercase letters, a hyphen, exactly three digits.
ID_RE = re.compile(r"\b([A-Z]{3,5})-([0-9]{3})\b(?!-?[0-9])")

# SPEC-034: a requirement is one row of a table with this header.
TABLE_HEADER_RE = re.compile(
    r"^\|\s*ID\s*\|\s*Title\s*\|\s*Requirement\s*\|\s*Why\s*\|\s*$"
)
REQUIREMENT_CELLS = 4  # ID, Title, Requirement, Why

# Any row whose first cell looks like an identifier. Used to catch a
# malformed id (SPEC-030) that ID_CELL_RE would silently skip.
ROW_LEAD_RE = re.compile(r"^\|\s*\**([A-Za-z]{2,8}-[0-9]+)\**\s*\|")

# The ID cell holds the identifier and nothing else.
ID_CELL_RE = re.compile(r"^([A-Z]{3,5}-[0-9]{3})$")

# A pipe that is not escaped separates cells; `\|` is a pipe inside a cell.
CELL_SPLIT_RE = re.compile(r"(?<!\\)\|")

# SPEC-050, SPEC-051. A link token lives in a comment. Requiring the comment
# leader stops a string literal such as `const N: &str = "verifies: JOIN-001";`
# from satisfying the evidence gate.
COMMENT_LEAD_RE = re.compile(r"^\s*(?://+|#+|--|/\*+|\*)\s*")
# Openers of multi-line string literals. Rust raw strings close on `"` plus
# the same number of hashes; triple quotes close on themselves.
RAW_OPEN_RE = re.compile(r"r(?P<hashes>#*)\"|\"\"\"|'''")
LINK_RE = re.compile(
    r"(implements|verifies):\s*"
    r"([A-Z]{3,5}-[0-9]{3}(?![0-9A-Za-z_-])"
    r"(?:\s*,\s*[A-Z]{3,5}-[0-9]{3}(?![0-9A-Za-z_-]))*)"
)

SECTION_RE = re.compile(r"^## (\d+)\. ")
FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---\n", re.DOTALL)

# SPEC-035. SHOULD is allowed only for an app or an operator.
SHOULD_ACTORS = (
    "an app",
    "an operator",
    "the app",
    "the operator",
    "apps",
    "operators",
)

SCAN_SUFFIXES = {
    ".rs",
    ".kt",
    ".kts",
    ".swift",
    ".ts",
    ".tsx",
    ".js",
    ".mjs",
    ".proto",
    ".py",
    ".sql",
}

SKIP_DIRS = {
    ".git",
    "target",
    "node_modules",
    "dist",
    "_site",
    "build",
    ".build",
    ".direnv",
    ".cache",
    ".worktrees",
    "generated",
    "vendor",
    ".gradle",
    ".swiftpm",
    "Pods",
    "__pycache__",
    ".ruff_cache",
    ".venv",
}

# Identifiers the format documentation uses to illustrate the scheme rather
# than to reference a real obligation. Kept explicit so that a code span is not
# a blanket exemption (SPEC-042).
EXAMPLE_IDS = {
    # Placeholders used to describe the scheme itself.
    "PREFIX-NNN",
    "PREFIX-MMM",
    # Illustrations in the format spec and the capability map. These name no
    # real obligation; they show a reader what an identifier looks like.
    "JOIN-012",
    "JOIN-013",
    "CONF-012",
    "JOIN-034",
    "JOIN-047",
}

# SPEC-078: a reference to an obligation nobody has written yet.
PENDING_REF_RE = re.compile(r"\?([A-Z]{3,5})\b")

# Documents that teach the scheme and may show sample identifiers.
EXAMPLE_FILES = {"SPEC-spec-format.md", "README.md"}

# Tokens that match the identifier shape but are not identifiers.
NOT_IDS = {"SHA-256", "SHA-512", "UTF-8", "BASE-64", "RFC-2119"}

# SPEC-032: a prefix inherited from a legacy document keeps that document's
# number space, so a new requirement never reuses a string that already meant
# something. See the floors table in specs/PREFIXES.md.
PREFIX_FLOORS = {"API": 200, "PUSH": 200}

MAX_REQUIREMENTS = 150  # SPEC-005
MAX_SENTENCES = 3  # SPEC-037

# Whether a missing verifies link fails the check. SPEC-058 keeps this at
# "warn" while backlinks are being added, and an owner flips it to "error"
# when the last approved spec has its links.
GATE_DEFAULT = "warn"


def split_row(line: str) -> list[str] | None:
    """Return the cells of a table row, or None when the line is not a row.

    A row starts and ends with a pipe. An escaped pipe stays inside its cell,
    and is unescaped here so the text reads as the author meant it.
    """
    parts = CELL_SPLIT_RE.split(line.strip())
    if len(parts) < 3 or parts[0].strip() or parts[-1].strip():
        return None
    return [c.strip().replace("\\|", "|") for c in parts[1:-1]]


@dataclass
class Requirement:
    id: str
    prefix: str
    number: int
    title: str
    text: str
    spec: str
    status: str
    section: str
    line: int
    why: str = ""
    should: bool = False
    implements: list[tuple[str, int]] = field(default_factory=list)
    verifies: list[tuple[str, int]] = field(default_factory=list)


@dataclass
class Finding:
    level: str  # "error" or "warning"
    where: str
    rule: str
    message: str

    def render(self) -> str:
        return f"{self.level}: {self.where}: [{self.rule}] {self.message}"


class Checker:
    def __init__(self, root: Path, gate: str = GATE_DEFAULT) -> None:
        self.root = root
        self.gate = gate
        self.findings: list[Finding] = []
        self.requirements: dict[str, Requirement] = {}
        self.prefixes: dict[str, dict[str, str]] = {}
        self.waivers: dict[str, dict] = {}
        self.spec_status: dict[str, str] = {}

    # -- reporting ---------------------------------------------------------

    def error(self, where: str, rule: str, message: str) -> None:
        self.findings.append(Finding("error", where, rule, message))

    def warn(self, where: str, rule: str, message: str) -> None:
        self.findings.append(Finding("warning", where, rule, message))

    # -- loading -----------------------------------------------------------

    def load_prefixes(self) -> None:
        """Read specs/PREFIXES.md. SPEC-031."""
        path = self.root / "specs" / "PREFIXES.md"
        if not path.exists():
            self.error(
                "specs/PREFIXES.md", "SPEC-031", "the prefix registry is missing"
            )
            return
        legacy = False
        registry = False
        for line in path.read_text().splitlines():
            if line.startswith("## "):
                heading = line[3:].strip().lower()
                # Only the two registry sections list prefixes; later sections
                # (such as the reuse floors) reference them.
                registry = heading in ("active", "legacy")
                legacy = heading == "legacy"
            if not registry:
                continue
            cells = [c.strip() for c in line.split("|")[1:-1]]
            if len(cells) < 2 or cells[0] in ("Prefix", "---"):
                continue
            prefix = cells[0].strip("`")
            if not re.fullmatch(r"[A-Z]{3,5}", prefix):
                continue
            if prefix in self.prefixes:
                self.error(
                    "specs/PREFIXES.md",
                    "SPEC-031",
                    f"prefix {prefix} is registered twice",
                )
            self.prefixes[prefix] = {
                "spec": cells[1],
                "file": cells[2].strip("`") if len(cells) > 2 else "",
                "legacy": legacy,
            }

    def load_waivers(self) -> None:
        """Read specs/waivers.toml. SPEC-055."""
        path = self.root / "specs" / "waivers.toml"
        if not path.exists():
            return
        try:
            data = tomllib.loads(path.read_text())
        except tomllib.TOMLDecodeError as exc:
            self.error("specs/waivers.toml", "SPEC-055", f"invalid TOML: {exc}")
            return
        for entry in data.get("waiver", []):
            rid = entry.get("id")
            reason = entry.get("reason")
            if not rid:
                self.error("specs/waivers.toml", "SPEC-055", "a waiver has no id")
                continue
            if not reason:
                self.error(
                    "specs/waivers.toml",
                    "SPEC-055",
                    f"the waiver for {rid} has no reason",
                )
            kind = entry.get("kind")
            if kind not in ("analysis", "gap"):
                self.error(
                    "specs/waivers.toml",
                    "SPEC-057",
                    f'the waiver for {rid} needs kind = "analysis" or "gap"',
                )
            elif kind == "gap" and not (entry.get("owner") and entry.get("issue")):
                self.error(
                    "specs/waivers.toml",
                    "SPEC-057",
                    f"the gap waiver for {rid} needs an owner and an issue",
                )
            self.waivers[rid] = {"reason": reason or "", "kind": kind}

    def load_specs(self) -> None:
        specs_dir = self.root / "specs"
        for path in sorted(specs_dir.glob("*.md")):
            if path.name in ("README.md", "GLOSSARY.md", "PREFIXES.md"):
                continue
            self.load_spec(path)

    def load_spec(self, path: Path) -> None:
        rel = path.relative_to(self.root).as_posix()
        raw = path.read_text()

        match = FRONTMATTER_RE.match(raw)
        if not match:
            self.error(rel, "SPEC-002", "no frontmatter")
            return
        meta = {}
        for line in match.group(1).splitlines():
            if ":" in line:
                key, _, value = line.partition(":")
                meta[key.strip()] = value.strip()
        extra = set(meta) - {"prefix", "status", "owns"}
        if extra:
            self.error(rel, "SPEC-002", f"unexpected frontmatter keys: {sorted(extra)}")
        prefix = meta.get("prefix", "")
        status = meta.get("status", "")
        if status not in ("draft", "approved", "legacy"):
            self.error(
                rel,
                "SPEC-002",
                f"status must be draft, approved, or legacy, not {status!r}",
            )
        if prefix not in self.prefixes:
            self.error(
                rel, "SPEC-031", f"prefix {prefix!r} is not in specs/PREFIXES.md"
            )
        self.spec_status[prefix] = status

        # SPEC-001: file name is PREFIX-slug.md
        stem = path.stem
        if not stem.startswith(f"{prefix}-"):
            self.error(
                rel, "SPEC-001", f"file name does not start with the prefix {prefix}-"
            )
        slug = stem[len(prefix) + 1 :] if stem.startswith(f"{prefix}-") else ""
        if slug and not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", slug):
            self.error(
                rel,
                "SPEC-001",
                f"slug {slug!r} is not lowercase words joined by hyphens",
            )

        # SPEC-075: a split moves an unchanged obligation without changing
        # its id, so a spec can own identifiers from another prefix. It
        # declares them here rather than in prose the checker cannot read.
        owns = {t.strip() for t in meta.get("owns", "").split(",") if t.strip()}
        body = raw[match.end() :]
        offset = raw[: match.end()].count("\n") + 1
        self.check_sections(rel, body, offset)
        self.parse_requirements(rel, prefix, status, body, offset, owns)

    def check_sections(self, rel: str, body: str, offset: int) -> None:
        """SPEC-003: Scope, Terms, numbered sections, optional Known limitations."""
        headings: list[tuple[int, str]] = []
        fence = None
        for i, line in enumerate(body.splitlines(), start=offset + 1):
            marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
            if marker:
                token = marker.group(1)
                if fence is None:
                    fence = token
                elif token[0] == fence[0] and len(token) >= len(fence):
                    fence = None
                continue
            if fence is None and line.startswith("## "):
                headings.append((i, line[3:].strip()))

        names = [h[1] for h in headings]
        if "Scope" not in names:
            self.error(rel, "SPEC-003", "no ## Scope section")
        if "Terms" not in names:
            self.error(rel, "SPEC-003", "no ## Terms section")
        if names[:2] != ["Scope", "Terms"] and len(names) >= 2:
            self.error(
                rel,
                "SPEC-003",
                f"sections must start with Scope then Terms, found {names[:2]}",
            )

        numbers = []
        for line_no, name in headings:
            m = SECTION_RE.match(f"## {name} ")
            if m:
                numbers.append((int(m.group(1)), line_no, name))
        for idx, (num, line_no, name) in enumerate(numbers, start=1):
            if num != idx:
                self.error(
                    f"{rel}:{line_no}",
                    "SPEC-003",
                    f"section numbered {num} is in position {idx}; numbers must ascend from 1",
                )
        if names and names[-1] not in ("Known limitations",) and numbers:
            last_numbered = numbers[-1][2]
            if names[-1] != last_numbered:
                self.error(
                    rel,
                    "SPEC-003",
                    f"the last section must be a numbered section or Known limitations, found {names[-1]!r}",
                )

    def check_webidl(self, rel: str, block: str, start_line: int) -> None:
        """SPEC-049, SPEC-070, SPEC-071: house rules for WebIDL blocks.

        This is a structural check, not a full parse: it catches the mistakes
        an author actually makes. A full parse needs webidl2.js and a Node
        dependency the default shell does not carry.
        """
        where = f"{rel}:{start_line}"
        stripped = re.sub(r"//[^\n]*", "", block)

        if stripped.count("{") != stripped.count("}"):
            self.error(where, "SPEC-049", "unbalanced braces in the WebIDL block")

        for kind, name in re.findall(
            r"\b(interface|callback|partial)\s+(\w*)", stripped
        ):
            self.error(
                where,
                "SPEC-049",
                f"`{kind}` is not allowed; an internal structure is a dictionary",
            )

        # SPEC-071: a union of two dictionary types is invalid WebIDL, because
        # dictionaries are never distinguishable from one another.
        dictionaries = set(re.findall(r"\bdictionary\s+(\w+)", stripped))
        for union in re.findall(r"\(([^()]*\bor\b[^()]*)\)", stripped):
            members = [m.strip().rstrip("?") for m in re.split(r"\bor\b", union)]
            named = [m for m in members if m in dictionaries]
            if len(named) > 1:
                self.error(
                    where,
                    "SPEC-071",
                    f"union ({union.strip()}) joins dictionary types, which WebIDL "
                    "cannot distinguish; use a discriminated wrapper",
                )

        # SPEC-070: opaque bytes are sequence<octet>, not a JS buffer type.
        for bad in re.findall(
            r"\b(ArrayBuffer|Uint8Array|ByteString|DataView)\b", stripped
        ):
            self.error(
                where,
                "SPEC-070",
                f"`{bad}` is a binding type; use sequence<octet> for opaque bytes",
            )

        for decl in re.findall(
            r"^[ \t]*((?:dictionary|enum|typedef)\b[^\n]*)", stripped, re.MULTILINE
        ):
            body = decl.strip()
            if body.endswith("{") or body.endswith(","):
                continue
            if body.startswith("typedef") and not body.endswith(";"):
                self.error(where, "SPEC-049", f"typedef is not terminated: {body!r}")

    def check_proto(self, rel: str, block: str, start_line: int) -> None:
        """SPEC-047, SPEC-048: house rules for inlined wire messages.

        A wire message is copied from the deployed schema, so the checks here
        are about the properties a spec can get wrong on its own: balanced
        braces, and a field number on every field. Agreement with the schema
        is a reviewer's job, not a parser's.
        """
        where = f"{rel}:{start_line}"
        stripped = re.sub(r"//[^\n]*", "", block)

        if stripped.count("{") != stripped.count("}"):
            self.error(where, "SPEC-047", "unbalanced braces in the proto block")

        for line in stripped.splitlines():
            text = line.strip()
            if not text or text.startswith(
                ("message", "enum", "oneof", "}", "reserved", "option", "//")
            ):
                continue
            if not text.endswith(";"):
                continue
            # An enum value is NAME = N; a field is `type name = N`.
            if "=" not in text:
                self.error(
                    where,
                    "SPEC-047",
                    f"field without a number: {text!r}",
                )

        # SPEC-047: field numbers are the compatibility contract, so a repeat
        # within one message is a real defect. A oneof shares its parent's
        # number space, and an enum with allow_alias may repeat deliberately.
        self.check_proto_numbers(where, stripped)

    def check_proto_numbers(self, where: str, text: str) -> None:
        for kind, name, body, depth in self.proto_scopes(text):
            if kind == "enum" and "allow_alias" in body and "true" in body:
                continue
            seen: dict[str, str] = {}
            for decl, num in re.findall(r"([\w.<>\s]+?)\s*=\s*(\d+)\s*;", body):
                field = decl.strip().split()[-1] if decl.strip() else "?"
                if field in ("option", "allow_alias"):
                    continue
                if num in seen:
                    self.error(
                        where,
                        "SPEC-047",
                        f"in {name}, number {num} is used by both "
                        f"{seen[num]} and {field}",
                    )
                else:
                    seen[num] = field

    @staticmethod
    def proto_scopes(text: str):
        """Yield (kind, name, body, depth) per message or enum.

        A message's body includes any `oneof` it contains, because a oneof
        shares the enclosing message's field-number space. A nested message
        gets its own scope and is removed from its parent's body.
        """
        opens = list(re.finditer(r"\b(message|enum|oneof)\s+(\w+)\s*\{", text))
        for m in opens:
            depth, i = 1, m.end()
            while i < len(text) and depth:
                if text[i] == "{":
                    depth += 1
                elif text[i] == "}":
                    depth -= 1
                i += 1
            body = text[m.end() : i - 1]
            if m.group(1) == "oneof":
                continue  # counted inside its parent
            # Drop nested message and enum bodies; they have their own scopes.
            inner = re.sub(r"\b(?:message|enum)\s+\w+\s*\{[^{}]*\}", "", body)
            yield m.group(1), m.group(2), inner, depth

    def finish_requirement(
        self,
        rel: str,
        prefix: str,
        status: str,
        pending: dict,
        section: str,
        owns: set[str] | None = None,
    ) -> int:
        """Validate one requirement row. Returns 1 if it counts."""
        rid = pending["id"]
        text = " ".join(pending["lines"]).strip()
        where = f"{rel}:{pending['line']}"
        rprefix, number = rid.split("-")

        owns = owns or set()
        inherited = rid in owns
        if rprefix != prefix and not inherited:
            self.error(
                where,
                "SPEC-031",
                f"{rid} does not use this spec's prefix {prefix}; declare it in "
                "`owns:` if a split moved it here",
            )
        # SPEC-032: a reuse floor stops a NEW requirement from taking a number
        # that already meant something. Inheriting the same obligation under
        # its original id is the case the floor exists to allow.
        floor = PREFIX_FLOORS.get(rprefix)
        if floor is not None and int(number) < floor and not inherited:
            self.error(
                where,
                "SPEC-032",
                f"{rid} is below the reuse floor {rprefix}-{floor:03d}; declare "
                "it in `owns:` to keep it for the same obligation",
            )
        if rid in self.requirements:
            other = self.requirements[rid]
            self.error(
                where,
                "SPEC-032",
                f"{rid} is already defined at {other.spec}:{other.line}",
            )

        req = Requirement(
            id=rid,
            prefix=rprefix,
            number=int(number),
            title=pending["title"],
            text=text,
            spec=rel,
            status=status,
            section=section,
            line=pending["line"],
            why=pending["why"] or "",
        )
        self.check_requirement_text(rel, req)
        self.requirements[rid] = req
        return 1

    def parse_requirements(
        self,
        rel: str,
        prefix: str,
        status: str,
        body: str,
        offset: int,
        owns: set[str] | None = None,
    ) -> None:
        current_section = ""
        fence = None
        count = 0
        in_table = False
        idl: list[str] | None = None
        idl_start = 0
        idl_lang = ""
        for i, line in enumerate(body.splitlines(), start=offset + 1):
            marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
            if marker:
                token = marker.group(1)
                if fence is None:
                    fence = token
                    lang = line.strip().lstrip("`~").strip()
                    if lang in ("webidl", "proto"):
                        idl, idl_start, idl_lang = [], i, lang
                elif token[0] == fence[0] and len(token) >= len(fence):
                    fence = None
                    if idl is not None:
                        block = "\n".join(idl)
                        if idl_lang == "webidl":
                            self.check_webidl(rel, block, idl_start)
                        else:
                            self.check_proto(rel, block, idl_start)
                        idl = None
                continue
            if fence is not None:
                if idl is not None:
                    idl.append(line)
                continue
            if line.startswith("## "):
                current_section = line[3:].strip()
                in_table = False
                continue
            if TABLE_HEADER_RE.match(line):
                in_table = True
                continue
            # A table ends at a blank line or at a line that is not a row.
            if not line.startswith("|"):
                in_table = False
                continue

            lead = ROW_LEAD_RE.match(line)
            if not lead:
                continue
            # SPEC-034: a requirement is one row, so there is nothing to
            # collect across lines. Everything the checks need is here.
            where = f"{rel}:{i}"
            if not in_table:
                self.error(
                    where,
                    "SPEC-034",
                    f"{lead.group(1)} is a row outside a requirements table; "
                    "the table header is `| ID | Title | Requirement | Why |`",
                )
            cells = split_row(line)
            if cells is None or len(cells) != REQUIREMENT_CELLS:
                self.error(
                    where,
                    "SPEC-034",
                    f"{lead.group(1)} needs exactly four cells: ID, Title, "
                    "Requirement, Why",
                )
                continue
            match = ID_CELL_RE.match(cells[0])
            if not match:
                # SPEC-030: catch a requirement-shaped row whose id is
                # malformed. Without this it would parse as an ordinary table
                # row and disappear from every check.
                self.error(
                    where,
                    "SPEC-034",
                    f"{cells[0]!r} is not a bare identifier of the form PREFIX-NNN",
                )
                continue
            pending = {
                "id": match.group(1),
                "title": cells[1].rstrip("."),
                "lines": [cells[2]],
                "why": cells[3],
                "line": i,
            }
            count += self.finish_requirement(
                rel, prefix, status, pending, current_section, owns
            )

        if count > MAX_REQUIREMENTS and prefix != "SPEC":
            self.error(
                rel,
                "SPEC-005",
                f"{count} requirements exceeds the ceiling of {MAX_REQUIREMENTS}; split the spec",
            )

    def check_requirement_text(self, rel: str, req: Requirement) -> None:
        where = f"{rel}:{req.line}"
        words = req.title.split()
        if not 2 <= len(words) <= 7:
            self.error(
                where,
                "SPEC-036",
                f"{req.id} title has {len(words)} words; use two to seven",
            )

        # A keyword inside a code span is named, not used: the format spec has
        # to be able to talk about `SHALL` and `SHOULD` without tripping these.
        text = req.text
        spoken = re.sub(r"`[^`]*`", "", text)

        if re.search(r"\bSHALL\b", spoken):
            self.error(where, "SPEC-035", f"{req.id} uses SHALL; use MUST")

        has_must = "MUST" in spoken
        has_may = re.search(r"\bMAY\b", spoken) is not None
        has_should = re.search(r"\bSHOULD\b", spoken) is not None
        if not (has_must or has_may or has_should):
            self.error(
                where, "SPEC-035", f"{req.id} has no MUST, MUST NOT, MAY, or SHOULD"
            )
        if has_should:
            # SPEC-035: SHOULD belongs to an app or an operator, and the actor
            # must be the one immediately before it. "When an app SHOULD retry,
            # the client MUST reject it" is a client MUST, not a SHOULD.
            lead = spoken[: spoken.index("SHOULD")].lower()
            actor_ok = any(lead.rstrip().endswith(a) for a in SHOULD_ACTORS)
            if not actor_ok:
                self.error(
                    where,
                    "SPEC-035",
                    f"{req.id} uses SHOULD, whose actor must be an app or an "
                    "operator named immediately before it",
                )
            # Only a pure SHOULD is exempt from evidence (SPEC-054). A MUST in
            # the same requirement is a real obligation and needs a test.
            if has_must:
                self.error(
                    where,
                    "SPEC-035",
                    f"{req.id} mixes SHOULD with MUST; split them so the "
                    "obligation is not exempted from evidence",
                )
            else:
                req.should = actor_ok

        # SPEC-034: one obligation per requirement. A checker cannot count
        # obligations, but "... MUST x and MUST y" is the shape authors reach
        # for when they are about to write two, so warn on it.
        # Two obligations usually show up as a second actor taking a second
        # MUST: "the client MUST x, and the backend MUST y". A second MUST that
        # continues the same actor's sentence is one obligation stated in parts.
        second_actor = re.search(
            r"\bMUST(?: NOT)?\b.*?\b(?:and|then)\b\s+"
            r"(?:the|a|an)\s+\w+(?:\s+\w+)?\s+MUST(?: NOT)?\b",
            spoken,
        )
        if second_actor:
            self.warn(
                where,
                "SPEC-034",
                f"{req.id} gives a second actor its own MUST; check that it "
                "states one obligation",
            )

        sentences = [s for s in re.split(r"(?<=[.!?]) +", text.strip()) if s]
        if len(sentences) > MAX_SENTENCES:
            self.warn(
                where,
                "SPEC-037",
                f"{req.id} has {len(sentences)} sentences; keep it to {MAX_SENTENCES}",
            )

    # -- scanning ----------------------------------------------------------

    def scan_code(self) -> None:
        for path in self.walk():
            rel = path.relative_to(self.root).as_posix()
            try:
                text = path.read_text(errors="replace")
            except OSError:
                continue
            if (
                "implements:" not in text
                and "verifies:" not in text
                and not ID_RE.search(text)
            ):
                continue
            # A token inside a multi-line string is not a comment. Track the
            # open-string state so `r#"..."#` and its friends cannot supply
            # evidence (SPEC-051).
            in_raw: str | None = None
            for i, line in enumerate(text.splitlines(), start=1):
                if in_raw is not None:
                    if in_raw in line:
                        in_raw = None
                    continue
                opener = RAW_OPEN_RE.search(line)
                if opener:
                    token = opener.group(0)
                    closer = (
                        '"' + opener.group("hashes") if token.startswith("r") else token
                    )
                    if closer not in line[opener.end() :]:
                        in_raw = closer
                        continue
                self.scan_line(rel, i, line)

    def walk(self):
        for dirpath, dirnames, filenames in os.walk(self.root):
            dirnames[:] = [
                d for d in dirnames if d not in SKIP_DIRS and not d.startswith(".")
            ]
            rel_dir = Path(dirpath).relative_to(self.root).as_posix()
            if rel_dir.startswith("specs") or rel_dir.startswith("docs/specs"):
                continue
            for name in filenames:
                if Path(name).suffix not in SCAN_SUFFIXES:
                    continue
                path = Path(dirpath) / name
                # The checker and its tests quote requirement ids as data.
                if path.relative_to(self.root).as_posix().startswith("dev/specs/"):
                    continue
                yield path

    def scan_line(self, rel: str, line_no: int, line: str) -> None:
        where = f"{rel}:{line_no}"
        linked: set[str] = set()
        # Only a comment carries a link token (SPEC-050, SPEC-051).
        comment = COMMENT_LEAD_RE.match(line)
        link_source = line[comment.end() :] if comment else ""
        for kind, ids in LINK_RE.findall(link_source):
            for rid in [s.strip() for s in ids.split(",")]:
                linked.add(rid)
                req = self.requirements.get(rid)
                if req is None:
                    prefix = rid.split("-")[0]
                    if prefix in self.prefixes:
                        self.error(
                            where,
                            "SPEC-053",
                            f"{kind}: {rid} does not resolve to a requirement",
                        )
                    else:
                        self.warn(
                            where,
                            "SPEC-031",
                            f"{kind}: {rid} uses an unregistered prefix",
                        )
                    continue
                if kind == "implements":
                    req.implements.append((rel, line_no))
                else:
                    req.verifies.append((rel, line_no))

        # SPEC-053: any other mention of an identifier is noise.
        for match in ID_RE.finditer(line):
            rid = match.group(0)
            if rid in linked:
                continue
            prefix = match.group(1)
            if prefix not in self.prefixes:
                continue
            req = self.requirements.get(rid)
            if self.prefixes[prefix].get("legacy"):
                # A legacy identifier is stale text, not a broken contract. It
                # is removed when its replacement spec is approved (SPEC-009),
                # so it stays a warning and never blocks an unrelated PR.
                self.warn(
                    where,
                    "SPEC-053",
                    f"{rid} refers to a legacy spec; remove it with that spec",
                )
                continue
            message = f"{rid} is mentioned outside an implements: or verifies: token"
            if req is not None and req.status == "approved":
                self.error(where, "SPEC-053", message)
            else:
                self.warn(where, "SPEC-053", message)

    # -- link rules --------------------------------------------------------

    def check_links(self) -> None:
        for rid, req in sorted(self.requirements.items()):
            where = f"{req.spec}:{req.line}"
            # SPEC-050, SPEC-052: link counts are not capped. An obligation can
            # be enforced and evidenced at several boundaries, and discarding
            # that evidence to satisfy a number would be worse than the noise.
            if req.verifies or req.status != "approved":
                continue
            # SPEC-054 and SPEC-081: the meta spec and SHOULD requirements need
            # no evidence and no waiver.
            if req.prefix == "SPEC" or req.should:
                continue
            if rid in self.waivers:
                continue
            message = f"{rid} has no verifies link and no waiver"
            if self.gate == "error":
                self.error(where, "SPEC-051", message)
            else:
                self.warn(where, "SPEC-051", message)

        for rid in sorted(self.waivers):
            if rid not in self.requirements:
                self.error(
                    "specs/waivers.toml",
                    "SPEC-055",
                    f"the waiver for {rid} does not name a current requirement",
                )
            elif (
                self.requirements[rid].verifies
                and self.waivers[rid].get("kind") == "analysis"
            ):
                # A gap waiver records that behaviour is missing. Evidence for
                # one path does not close it, so only an analysis waiver is
                # made redundant by a link.
                self.warn(
                    "specs/waivers.toml",
                    "SPEC-055",
                    f"{rid} now has a verifies link; remove its analysis waiver",
                )

    # -- cross references --------------------------------------------------

    def check_pending_refs(self) -> None:
        """SPEC-078, SPEC-079: report every ?PREFIX marker and whose debt it is."""
        for path in sorted((self.root / "specs").glob("*.md")):
            if path.name in ("PREFIXES.md", "README.md"):
                continue
            rel = path.relative_to(self.root).as_posix()
            raw = path.read_text()
            own_status = "draft"
            match = FRONTMATTER_RE.match(raw)
            if match:
                for line in match.group(1).splitlines():
                    if line.startswith("status:"):
                        own_status = line.split(":", 1)[1].strip()
            fence = None
            for i, line in enumerate(raw.splitlines(), start=1):
                marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
                if marker:
                    token = marker.group(1)
                    if fence is None:
                        fence = token
                    elif token[0] == fence[0] and len(token) >= len(fence):
                        fence = None
                    continue
                if fence is not None:
                    continue
                for m in PENDING_REF_RE.finditer(line):
                    target = m.group(1)
                    if target not in self.prefixes:
                        self.error(
                            f"{rel}:{i}",
                            "SPEC-078",
                            f"?{target} names a prefix that is not registered",
                        )
                        continue
                    owner_status = self.spec_status.get(target)
                    if owner_status == "approved":
                        # The obligation exists now, so the debt is due.
                        self.error(
                            f"{rel}:{i}",
                            "SPEC-079",
                            f"?{target} is unresolved although {target} is "
                            "approved; replace it with the requirement id",
                        )
                    elif own_status == "approved":
                        self.error(
                            f"{rel}:{i}",
                            "SPEC-079",
                            f"this spec is approved but still carries ?{target}",
                        )
                    else:
                        self.warn(
                            f"{rel}:{i}",
                            "SPEC-078",
                            f"?{target} waits on the {target} spec",
                        )

    def check_cross_references(self) -> None:
        """SPEC-042: a referenced identifier must exist.

        An identifier in a code span or a fenced block is an illustration, not
        a reference: the format spec and the README both show sample ids such
        as `JOIN-012` to explain the scheme.
        """
        for path in sorted((self.root / "specs").glob("*.md")):
            # The registry describes the number space itself, so the ids in it
            # are boundaries, not references to obligations.
            if path.name == "PREFIXES.md":
                continue
            rel = path.relative_to(self.root).as_posix()
            fence = None
            for i, line in enumerate(path.read_text().splitlines(), start=1):
                marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
                if marker:
                    token = marker.group(1)
                    if fence is None:
                        fence = token
                    elif token[0] == fence[0] and len(token) >= len(fence):
                        fence = None
                    continue
                if fence is not None:
                    continue
                # A requirement's own id sits in its first cell and is not a
                # reference; scan the other cells.
                cells = split_row(line) if ROW_LEAD_RE.match(line) else None
                scan = " | ".join(cells[1:]) if cells else line
                # A code span is a reference like any other: `JOIN-012` in a
                # requirement still points at an obligation. Only the meta
                # spec's own illustrations are exempt, and it says so.
                for m in ID_RE.finditer(scan):
                    rid = m.group(0)
                    if rid in self.requirements:
                        continue
                    prefix = m.group(1)
                    if rid in NOT_IDS:
                        continue
                    # An illustration is only an illustration in the documents
                    # that teach the scheme. Elsewhere it is a real reference.
                    if rid in EXAMPLE_IDS and path.name in EXAMPLE_FILES:
                        continue
                    if prefix not in self.prefixes:
                        self.error(
                            f"{rel}:{i}",
                            "SPEC-042",
                            f"reference to {rid}, whose prefix is not registered",
                        )
                        continue
                    if self.prefixes[prefix].get("legacy"):
                        continue
                    self.error(
                        f"{rel}:{i}",
                        "SPEC-042",
                        f"reference to {rid}, which is not a current requirement",
                    )

    def run(self) -> None:
        self.load_prefixes()
        self.load_waivers()
        self.load_specs()
        self.scan_code()
        self.check_links()
        self.check_cross_references()
        self.check_pending_refs()

    # -- output ------------------------------------------------------------

    def report(self) -> int:
        errors = [f for f in self.findings if f.level == "error"]
        warnings = [f for f in self.findings if f.level == "warning"]
        for finding in errors + warnings:
            print(finding.render())
        approved = sum(1 for r in self.requirements.values() if r.status == "approved")
        linked = sum(1 for r in self.requirements.values() if r.verifies)
        print(
            f"\n{len(self.requirements)} requirements in "
            f"{len({r.spec for r in self.requirements.values()})} specs "
            f"({approved} approved, {linked} with evidence, {len(self.waivers)} waived)"
        )
        print(f"{len(errors)} error(s), {len(warnings)} warning(s)")
        return 1 if errors else 0

    def index(self, as_json: bool) -> int:
        rows = [
            {
                "id": r.id,
                "title": r.title,
                "spec": r.spec,
                "status": r.status,
                "section": r.section,
                "implements": [f"{p}:{n}" for p, n in r.implements],
                "verifies": [f"{p}:{n}" for p, n in r.verifies],
                "waived": r.id in self.waivers,
            }
            for r in sorted(
                self.requirements.values(), key=lambda r: (r.prefix, r.number)
            )
        ]
        if as_json:
            print(json.dumps(rows, indent=2))
            return 0
        width = max((len(r["id"]) for r in rows), default=2)
        for row in rows:
            evidence = ", ".join(row["verifies"]) or (
                "waived" if row["waived"] else "-"
            )
            print(f"{row['id']:<{width}}  {row['title']:<34}  {evidence}")
        return 0

    def show(self, rid: str) -> int:
        req = self.requirements.get(rid)
        if req is None:
            print(f"{rid} is not a current requirement", file=sys.stderr)
            near = [r for r in self.requirements if r.startswith(rid.split("-")[0])]
            if near:
                print(
                    f"known ids for that prefix: {', '.join(sorted(near)[:8])}",
                    file=sys.stderr,
                )
            return 1
        print(f"{req.id} {req.title}")
        print(f"  spec:    {req.spec}:{req.line}  ({req.status})")
        print(f"  section: {req.section}")
        print(f"\n  {req.text}")
        if req.why:
            print(f"\n  Why: {req.why}")
        if req.implements:
            print("\n  implements:")
            for path, line in req.implements:
                print(f"    {path}:{line}")
        if req.verifies:
            print("\n  verifies:")
            for path, line in req.verifies:
                print(f"    {path}:{line}")
        elif req.id in self.waivers:
            print(
                f"\n  waived ({self.waivers[req.id]['kind']}): "
                f"{self.waivers[req.id]['reason']}"
            )
        return 0


def repo_root() -> Path:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        )
        return Path(out.stdout.strip())
    except (subprocess.CalledProcessError, FileNotFoundError):
        return Path.cwd()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "index", "show"))
    parser.add_argument("id", nargs="?", help="requirement id, for show")
    parser.add_argument("--json", action="store_true", help="machine-readable index")
    parser.add_argument(
        "--gate",
        choices=("warn", "error"),
        default=GATE_DEFAULT,
        help="whether a missing verifies link fails the check",
    )
    parser.add_argument("--root", type=Path, default=None)
    args = parser.parse_args(argv)

    checker = Checker(args.root or repo_root(), gate=args.gate)
    checker.run()

    if args.command == "check":
        return checker.report()
    if args.command == "index":
        return checker.index(args.json)
    if not args.id:
        parser.error("show needs a requirement id")
    return checker.show(args.id)


if __name__ == "__main__":
    sys.exit(main())
