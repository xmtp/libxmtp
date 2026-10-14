"""Tests for the spec checker.

Each case builds a small repository in a temporary directory, so the suite
never depends on the state of docs/specs/ in the real tree. Cases that differ
only in their input sit in one table test, one subTest each.
"""

from __future__ import annotations

import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

import check

REGISTRY = """# Prefix registry

## Active

| Prefix | Spec | File |
| --- | --- | --- |
| `JOIN` | Joining groups | `docs/specs/JOIN-joining-groups.md` |
| `API` | Backend API | `docs/specs/API-backend-api.md` |

## Legacy

| Prefix | Replaced by | Why it is still listed |
| --- | --- | --- |
| `CFG` | `CONF` | a stale mention remains |
"""

HEAD = """---
prefix: JOIN
status: {status}
---
# Joining groups

A summary paragraph.

## Scope

What is in and out.

## Terms

| Term | Meaning |
| --- | --- |
| Welcome | The message that adds an installation. |

## 1. Welcomes

Prose that explains the mechanism.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
"""

# The body of a spec file with a requirements table, after its front matter.
BODY = (
    "# T\n\nS.\n\n## Scope\n\nx\n\n## Terms\n\nx\n\n## 1. A\n\nProse.\n\n"
    "| ID | Title | Requirement | Why |\n| --- | --- | --- | --- |\n"
)
ROW = "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n"
API_A = REGISTRY.replace(
    "| `API` | Backend API | `docs/specs/API-backend-api.md` |",
    "| `API` | Backend API | `docs/specs/API-a.md` |",
)
GMOD = REGISTRY + "| `GMOD` | Modifying groups | `docs/specs/GMOD-g.md` |\n"
PENDING_REGISTRY = (
    "# Prefix registry\n\n## Active\n\n| Prefix | Spec | File |\n| --- | --- | --- |\n"
    "| `JOIN` | Joining groups | `docs/specs/JOIN-j.md` |\n"
    "| `IDENT` | Identity updates | `docs/specs/IDENT-i.md` |\n"
)


def front(prefix: str, status: str = "draft", extra: str = "") -> str:
    return f"---\nprefix: {prefix}\nstatus: {status}\n{extra}---\n"


def proto(body: str) -> str:
    return f"\n```proto\n{body}\n```\n"


def webidl(body: str) -> str:
    return f"\n```webidl\n{body}\n```\n"


def checked(
    rows: str = ROW,
    *,
    status: str = "approved",
    registry: str = REGISTRY,
    extra: str = "",
    specs: dict[str, str] | None = None,
    code: str | dict[str, str] = "",
    waivers: str | None = None,
    gate: str = "warn",
) -> check.Checker:
    """Run the checker on a temporary repository.

    `specs` replaces the one JOIN spec with these files. `code` is the body of
    src/lib.rs, or a map of file names to bodies.
    """
    with TemporaryDirectory() as d:
        tmp = Path(d)
        (tmp / "docs" / "specs").mkdir(parents=True)
        (tmp / "docs" / "specs" / "PREFIXES.md").write_text(registry)
        if specs is None:
            specs = {
                "JOIN-joining-groups.md": HEAD.format(status=status) + rows + extra
            }
        for name, text in specs.items():
            (tmp / "docs" / "specs" / name).write_text(text)
        files = {"src/lib.rs": code} if isinstance(code, str) else code
        for name, body in files.items():
            if body:
                (tmp / name).parent.mkdir(parents=True, exist_ok=True)
                (tmp / name).write_text(body)
        if waivers is not None:
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                "[[waiver]]\n" + waivers
            )
        checker = check.Checker(tmp, gate=gate)
        checker.run()
        return checker


def rules(checker: check.Checker, level: str | None = None) -> list[str]:
    return [f.rule for f in checker.findings if level is None or f.level == level]


def verifies(checker: check.Checker, ident: str = "JOIN-001") -> list:
    return checker.requirements[ident].verifies


class Table(unittest.TestCase):
    def assert_findings(self, cases):
        """Each case is (label, checker arguments, expectations).

        An expectation is `level:RULE` (reported), `!level:RULE` (not
        reported), or `clean` (no error at all).
        """
        for label, arguments, wants in cases:
            with self.subTest(label):
                checker = checked(**arguments)
                for want in wants:
                    if want == "clean":
                        self.assertEqual(rules(checker, "error"), [])
                        continue
                    level, rule = want.lstrip("!").split(":")
                    found = rule in rules(checker, level)
                    self.assertEqual(found, not want.startswith("!"), want)


def row(text: str, title: str = "Stale welcome") -> str:
    return f"| JOIN-001 | {title} | {text} | |\n"


# A Requirement cell, its title when not "Stale welcome", and the expectations.
ROW_CASES = [
    ("The client MUST discard it.", None, "clean"),
    ("The client MUST discard it.", "Welcomes", "error:SPEC-036"),
    ("The client SHALL discard it.", None, "error:SPEC-035"),
    (
        "When a Welcome arrives late, the client SHALL discard it.",
        None,
        "error:SPEC-035",
    ),
    # The whole Requirement cell is the obligation.
    (
        "When a Welcome arrives late, the client MUST discard it.",
        None,
        "!error:SPEC-035",
    ),
    # The format spec must be able to name SHALL without using it.
    ("A spec MUST NOT use `SHALL` here.", None, "clean"),
    ("The client SHOULD discard it.", None, "error:SPEC-035"),
    ("The client SHOULD notify the app.", None, "error:SPEC-035"),
    # A MUST must not inherit the SHOULD evidence exemption.
    ("When an app SHOULD retry, the client MUST discard it.", None, "error:SPEC-035"),
    ("An app SHOULD NOT persist the secret.", "Secret storage", "clean"),
    ("The client discards it.", None, "error:SPEC-035"),
    (
        "The client MUST discard it. One more. Two more. Three more.",
        None,
        "warning:SPEC-037 clean",
    ),
    # A cell may quote a row template whose periods belong to the example.
    (
        "A requirement MUST be one row in the form "
        "`\\| PREFIX-NNN \\| Title \\| Sentence. \\| Reason. \\|`. "
        "A cell MUST NOT contain a list.",
        "Row form",
        "!warning:SPEC-037 clean",
    ),
    # SPEC-034 is unenforceable exactly; this shape signals two actors.
    (
        "The client MUST discard it, and the backend MUST reject it.",
        None,
        "warning:SPEC-034 !error:SPEC-034",
    ),
    ("The client MUST discard it and MUST NOT report it.", None, "!warning:SPEC-034"),
    ("The client MUST discard it per API-900.", None, "error:SPEC-042"),
    # A code span is still a reference.
    ("The client MUST follow `API-900`.", None, "error:SPEC-042"),
    ("The client MUST follow XYZ-900.", None, "error:SPEC-042"),
    ("The client MUST comply with JOIN-047.", None, "error:SPEC-042"),
    ("The client MUST use SHA-256.", None, "!error:SPEC-042"),
    ("The client MUST verify it as ?NOPE requires.", "Proof check", "error:SPEC-078"),
    (
        "When a Welcome is late and unreadable, the client MUST discard it.",
        None,
        "!warning:SPEC-034",
    ),
]


class Requirements(Table):
    def test_requirement_rows_are_checked(self):
        self.assert_findings(
            [
                (text, {"rows": row(text, title or "Stale welcome")}, wants.split())
                for text, title, wants in ROW_CASES
            ]
            + [
                ("duplicate id", {"rows": ROW + ROW}, ["error:SPEC-032"]),
                (
                    "malformed bullet",
                    {"rows": "| JOIN-001 | The client MUST discard it. | |\n"},
                    ["error:SPEC-034"],
                ),
                # SPEC-087: owned values bind without a pointing requirement.
                (
                    "value table needs no wrapper requirement",
                    {
                        "extra": "\n| Extension | Identifier |\n| --- | --- |\n| Wrapper | `0xff03` |\n"
                    },
                    ["clean"],
                ),
            ]
        )

    def test_cells_and_ids_are_parsed(self):
        why = "| JOIN-001 | Stale welcome | The client MUST discard it. | An older Welcome would roll the member back. |\n"
        self.assertIn("roll the member back", checked(why).requirements["JOIN-001"].why)
        self.assertEqual(checked().requirements["JOIN-001"].why, "")
        pipe = checked(row("The client MUST write `a \\| b` as shown."))
        self.assertIn("a | b", pipe.requirements["JOIN-001"].text)
        self.assertEqual(rules(pipe, "error"), [])
        ended = checked(ROW + "\nOrdinary prose that mentions SHALL as a word.\n")
        self.assertEqual(
            ended.requirements["JOIN-001"].text, "The client MUST discard it."
        )
        self.assertEqual(rules(ended, "error"), [])
        fenced = (
            ROW
            + "\n```markdown\n| JOIN-999 | Example only | The client MUST do nothing. | |\n```\n"
        )
        self.assertEqual(list(checked(fenced).requirements), ["JOIN-001"])
        # A malformed id must be reported, not skipped by every check.
        for rows in (
            "| **JOIN-001** | Stale welcome | The client MUST discard it. | |\n",
            "| JOIN-001 | Stale welcome | The client MUST discard it. |\n",
            "| JOIN-01 | Bad id | The client MUST discard it. | |\n",
            "| JOIN-1000 | Wide id | The client MUST discard it. | |\n",
        ):
            with self.subTest(rows=rows):
                checker = checked(rows)
                self.assertEqual(checker.requirements, {})
                self.assertIn("SPEC-034", rules(checker, "error"))
        # A requirement pasted into another table is still parsed, and reported.
        other = checked(
            "\n| Term | Meaning | Note | More |\n| --- | --- | --- | --- |\n" + ROW
        )
        self.assertIn("SPEC-034", rules(other, "error"))
        self.assertIn("JOIN-001", other.requirements)


class Structure(Table):
    def test_structure_and_references_are_checked(self):
        empty_registry = (
            "# Prefix registry\n\n| Prefix | Spec | File |\n| --- | --- | --- |\n"
        )
        ceiling = "".join(
            f"| JOIN-{n:03d} | Rule number {n} | The client MUST do it. | |\n"
            for n in range(1, check.MAX_REQUIREMENTS + 2)
        )
        moved = BODY + "| JOIN-001 | Moved rule | The client MUST reject it. | |\n"
        self.assert_findings(
            [
                (
                    "unregistered prefix",
                    {"registry": empty_registry},
                    ["error:SPEC-031"],
                ),
                (
                    "missing terms section",
                    {
                        "specs": {
                            "JOIN-joining-groups.md": front("JOIN")
                            + "# T\n\nSummary.\n\n## Scope\n\nIn and out.\n\n## 1. Welcomes\n\nProse.\n"
                        }
                    },
                    ["error:SPEC-003"],
                ),
                (
                    "out-of-order sections",
                    {"extra": "\n## 3. Later\n\nProse.\n"},
                    ["error:SPEC-003"],
                ),
                ("requirement ceiling", {"rows": ceiling}, ["error:SPEC-005"]),
                (
                    "bad front matter key",
                    {
                        "specs": {
                            "JOIN-joining-groups.md": front(
                                "JOIN", extra="owner: someone\n"
                            )
                            + BODY
                        }
                    },
                    ["error:SPEC-002"],
                ),
                # A number that already meant a legacy requirement cannot be reallocated.
                (
                    "reused prefix below the floor",
                    {
                        "registry": API_A,
                        "specs": {
                            "API-a.md": front("API")
                            + BODY
                            + "| API-001 | Topic derivation | The backend MUST derive it. | |\n"
                        },
                    },
                    ["error:SPEC-032"],
                ),
                (
                    "reused prefix above the floor",
                    {
                        "registry": API_A,
                        "specs": {
                            "API-a.md": front("API")
                            + BODY
                            + "| API-200 | Topic derivation | The backend MUST derive it. | |\n"
                        },
                    },
                    ["!error:SPEC-032"],
                ),
                (
                    "moved id without owns",
                    {"registry": GMOD, "specs": {"GMOD-g.md": front("GMOD") + moved}},
                    ["error:SPEC-031"],
                ),
                (
                    "moved id with owns",
                    {
                        "registry": GMOD,
                        "specs": {
                            "GMOD-g.md": front("GMOD", extra="owns: JOIN-001\n") + moved
                        },
                    },
                    ["!error:SPEC-031"],
                ),
            ]
        )

    def test_pending_references_follow_both_statuses(self):
        def pending(join_status, ident_status):
            checker = checked(
                registry=PENDING_REGISTRY,
                specs={
                    "JOIN-j.md": front("JOIN", join_status)
                    + BODY
                    + "| JOIN-001 | Proof check | The client MUST verify it as ?IDENT requires. | |\n",
                    "IDENT-i.md": front("IDENT", ident_status)
                    + BODY
                    + "| IDENT-001 | Proof rule | The client MUST check it. | |\n",
                },
            )
            return [
                (f.level, f.rule)
                for f in checker.findings
                if f.rule in ("SPEC-078", "SPEC-079")
            ]

        self.assertEqual(pending("draft", "draft"), [("warning", "SPEC-078")])
        # It blocks approving the referring spec, and is due once the owner is approved.
        self.assertIn(("error", "SPEC-079"), pending("approved", "draft"))
        self.assertIn(("error", "SPEC-079"), pending("draft", "approved"))


class Links(Table):
    def test_evidence_links_are_collected(self):
        cases = [
            ("one verifies link", "// verifies: JOIN-001\nfn test_welcome() {}\n", 1),
            # Evidence at several boundaries is not capped.
            (
                "many verifies links",
                "".join(f"// verifies: JOIN-001\nfn t{n}() {{}}\n" for n in range(5)),
                5,
            ),
            (
                "SDK language",
                {
                    "sdks/ios/Tests/WelcomeTests.swift": "// verifies: JOIN-001\nfunc testWelcome() {}\n"
                },
                1,
            ),
            (
                "skipped directory",
                {"target/debug/gen.rs": "// verifies: JOIN-001\nfn f() {}\n"},
                0,
            ),
            # A link token only counts inside a comment.
            ("string literal", 'const NOTE: &str = "verifies: JOIN-001";\n', 0),
            ("raw string", 'const DOC: &str = r#"\n// verifies: JOIN-001\n"#;\n', 0),
            ("malformed link token", "// verifies: JOIN-0010\nfn t() {}\n", 0),
            ("hash comment", {"src/b.py": "# verifies: JOIN-001\nx\n"}, 1),
            ("dash comment", {"src/c.sql": "-- verifies: JOIN-001\nx\n"}, 1),
        ]
        for label, code, count in cases:
            with self.subTest(label):
                checker = checked(code=code)
                self.assertEqual(len(verifies(checker)), count)
                if label.endswith("links"):
                    self.assertEqual(rules(checker, "error"), [])
        # In Kotlin, `r"` can end a string such as `"$error"`; it opens nothing.
        kotlin = (
            'val text = "failed: $error"\n\n// verifies: JOIN-001\nfun check() {}\n'
        )
        self.assertEqual(
            verifies(checked(code={"src/Check.kt": kotlin})), [("src/Check.kt", 3)]
        )
        two = ROW + "| JOIN-002 | Fresh welcome | The client MUST accept it. | |\n"
        checker = checked(two, code="// verifies: JOIN-001, JOIN-002\nfn f() {}\n")
        self.assertEqual(rules(checker, "error"), [])
        self.assertEqual(len(verifies(checker, "JOIN-002")), 1)

    def test_link_findings(self):
        legacy = "// CFG-051: the identifier is the binding\nfn f() {}\n"
        linked = "// verifies: JOIN-001\nfn t() {}\n"
        analysis = 'id = "JOIN-001"\nkind = "analysis"\nreason = "reviewed by hand"\n'
        gap = 'id = "JOIN-001"\nkind = "gap"\nreason = "not built"\n'
        untested = 'id = "JOIN-001"\nkind = "untested"\nreason = "no test"\n'
        issue = 'owner = "backend"\nissue = "https://example.invalid/1"\n'
        # A waiver alone, and the finding it must or must not raise.
        waivers = [
            ('id = "JOIN-001"\nkind = "analysis"\n', "error:SPEC-055"),
            ('id = "JOIN-777"\nkind = "analysis"\nreason = "typo"\n', "error:SPEC-055"),
            ('id = "JOIN-001"\nreason = "later"\n', "error:SPEC-094"),
            (gap, "error:SPEC-095"),
            (gap + 'owner = "backend"\n', "error:SPEC-095"),
            (gap + issue, "!error:SPEC-095"),
            (untested, "error:SPEC-095"),
            (untested + 'owner = "protocol"\n', "!error:SPEC-095"),
        ]
        self.assert_findings(
            [
                (
                    "many implements links",
                    {
                        "code": "".join(
                            f"// implements: JOIN-001\nfn f{n}() {{}}\n"
                            for n in range(4)
                        )
                        + linked
                    },
                    ["clean"],
                ),
                (
                    "stray mention of an approved id",
                    {"code": "// see JOIN-001 for the rule\nfn f() {}\n"},
                    ["error:SPEC-053"],
                ),
                # Legacy ids are cleaned up with their spec, so they must not block a PR.
                (
                    "stray legacy mention",
                    {"code": legacy},
                    ["warning:SPEC-053", "clean"],
                ),
                # A renamed section must not unregister its prefixes silently.
                (
                    "retired section registers prefixes",
                    {
                        "code": legacy,
                        "registry": REGISTRY.replace("## Legacy", "## Retired"),
                    },
                    ["warning:SPEC-053", "clean"],
                ),
                (
                    "unknown id with a known prefix",
                    {"code": "// verifies: JOIN-404\nfn f() {}\n"},
                    ["error:SPEC-053"],
                ),
                ("missing evidence warns", {}, ["warning:SPEC-051"]),
                ("the gate makes it an error", {"gate": "error"}, ["error:SPEC-051"]),
                (
                    "draft needs no evidence",
                    {"gate": "error", "status": "draft"},
                    ["!error:SPEC-051"],
                ),
                (
                    "a waiver satisfies the gate",
                    {"gate": "error", "waivers": analysis},
                    ["!error:SPEC-051"],
                ),
                (
                    "stale waiver",
                    {"waivers": analysis, "code": linked},
                    ["warning:SPEC-055"],
                ),
                (
                    "stale untested waiver",
                    {"waivers": untested + 'owner = "protocol"\n', "code": linked},
                    ["warning:SPEC-055"],
                ),
            ]
            + [(waiver, {"waivers": waiver}, [want]) for waiver, want in waivers]
        )
        # Evidence for one path does not close a recorded implementation gap.
        checker = checked(waivers=gap + issue, code=linked)
        self.assertEqual(
            [
                f
                for f in checker.findings
                if f.rule == "SPEC-055" and "remove" in f.message
            ],
            [],
        )


# A proto or WebIDL block, and the expectations.
BLOCK_CASES = [
    (
        proto(
            "message WelcomeMessage {\n  bytes installation_key = 1;  // 32 bytes\n  bytes data = 2;\n}"
        ),
        "clean",
    ),
    # The shape WebIDL cannot express is the normal case on the wire.
    (
        proto(
            "message WelcomeMessage {\n  oneof version {\n    V1 v1 = 1;\n    WelcomePointer welcome_pointer = 2;\n  }\n}"
        ),
        "clean",
    ),
    (proto("message Welcome {\n  bytes data;\n}"), "error:SPEC-047"),
    (proto("message Welcome {\n  bytes data = 1;"), "error:SPEC-047"),
    (
        proto("message Welcome {\n  bytes data = 1;\n  bytes other = 1;\n}"),
        "error:SPEC-047",
    ),
    (
        proto(
            "message Sample {\n  bytes id = 1;\n  oneof payload {\n    bytes data = 1;\n  }\n}"
        ),
        "error:SPEC-047",
    ),
    (
        proto(
            "enum State {\n  option allow_alias = true;\n  UNKNOWN = 0;\n  STARTED = 1;\n  RUNNING = 1;\n}"
        ),
        "!error:SPEC-047",
    ),
    (
        proto(
            "message Outer {\n  message Inner {\n    bytes a = 1;\n  }\n  bytes b = 1;\n}"
        ),
        "!error:SPEC-047",
    ),
    # `bytes` is correct in proto; sequence<octet> is a WebIDL-only rule.
    (proto("message Welcome {\n  bytes data = 1;\n}"), "!error:SPEC-070 clean"),
    (
        webidl(
            "dictionary Welcome {\n  required sequence<octet> installation_key;   // 32 bytes\n};"
        ),
        "clean",
    ),
    (webidl("typedef unsigned long Counter;"), "clean"),
    (webidl("interface Welcome {\n  void send();\n};"), "error:SPEC-049"),
    # WebIDL cannot distinguish two dictionary types in a union.
    (
        webidl(
            "dictionary WelcomeMessage { required sequence<octet> data; };\n"
            "dictionary WelcomePointer { required sequence<octet> pointer; };\n"
            "typedef (WelcomeMessage or WelcomePointer) Welcome;"
        ),
        "error:SPEC-071",
    ),
    (webidl("dictionary Welcome {\n  required Uint8Array data;\n};"), "error:SPEC-070"),
    (
        webidl("dictionary Welcome {\n  required sequence<octet> data;"),
        "error:SPEC-049",
    ),
    (
        webidl(
            "dictionary Welcome {\n  required sequence<octet> data;   // not a Uint8Array\n};"
        ),
        "!error:SPEC-070",
    ),
]


class Blocks(Table):
    def test_proto_and_webidl_blocks_are_checked(self):
        self.assert_findings(
            [(block, {"extra": block}, wants.split()) for block, wants in BLOCK_CASES]
        )


if __name__ == "__main__":
    unittest.main()
