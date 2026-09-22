"""Tests for the spec checker.

Each test builds a small repository in a temporary directory, so the suite
never depends on the state of specs/ in the real tree.
"""

from __future__ import annotations

import textwrap
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


def build(
    tmp: Path,
    requirements: str,
    *,
    status: str = "approved",
    registry: str = REGISTRY,
    extra: str = "",
) -> Path:
    (tmp / "docs" / "specs").mkdir(parents=True, exist_ok=True)
    (tmp / "docs" / "specs" / "PREFIXES.md").write_text(registry)
    (tmp / "docs" / "specs" / "JOIN-joining-groups.md").write_text(
        HEAD.format(status=status) + requirements + extra
    )
    return tmp


def run(tmp: Path, gate: str = "warn") -> check.Checker:
    checker = check.Checker(tmp, gate=gate)
    checker.run()
    return checker


def rules(checker: check.Checker, level: str | None = None) -> list[str]:
    return [f.rule for f in checker.findings if level is None or f.level == level]


class RequirementForm(unittest.TestCase):
    def test_well_formed_requirement_passes(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            checker = run(tmp)
            self.assertEqual(rules(checker, "error"), [])
            self.assertIn("JOIN-001", checker.requirements)

    def test_duplicate_id_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                textwrap.dedent("""\
                | JOIN-001 | Stale welcome | The client MUST discard it. | |
                | JOIN-001 | Other rule | The client MUST accept it. | |
                """),
            )
            self.assertIn("SPEC-032", rules(run(tmp), "error"))

    def test_one_word_title_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d), "| JOIN-001 | Welcomes | The client MUST discard it. | |\n"
            )
            self.assertIn("SPEC-036", rules(run(tmp), "error"))

    def test_shall_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client SHALL discard it. | |\n",
            )
            self.assertIn("SPEC-035", rules(run(tmp), "error"))

    def test_backticked_keyword_is_not_a_use(self):
        """The format spec must be able to name SHALL without using it."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | A spec MUST NOT use `SHALL` here. | |\n",
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_should_needs_an_unenforceable_actor(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client SHOULD discard it. | |\n",
            )
            self.assertIn("SPEC-035", rules(run(tmp), "error"))

    def test_should_is_allowed_for_an_app(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Secret storage | An app SHOULD NOT persist the secret. | |\n",
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_missing_keyword_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d), "| JOIN-001 | Stale welcome | The client discards it. | |\n"
            )
            self.assertIn("SPEC-035", rules(run(tmp), "error"))

    def test_long_requirement_is_a_warning(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                (
                    "| JOIN-001 | Stale welcome | The client MUST discard it. "
                    "One more. Two more. Three more. | |\n"
                ),
            )
            checker = run(tmp)
            self.assertIn("SPEC-037", rules(checker, "warning"))
            self.assertEqual(rules(checker, "error"), [])

    def test_periods_inside_code_do_not_count_as_sentences(self):
        # A cell may quote a row template whose periods belong to the example.
        # Counting them as sentence ends made SPEC-034 report itself.
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                (
                    "| JOIN-001 | Row form | A requirement MUST be one row in the "
                    "form `\\| PREFIX-NNN \\| Title \\| Sentence. \\| Reason. \\|`. "
                    "A cell MUST NOT contain a list. | |\n"
                ),
            )
            checker = run(tmp)
            self.assertNotIn("SPEC-037", rules(checker, "warning"))
            self.assertEqual(rules(checker, "error"), [])

    def test_malformed_bullet_is_reported(self):
        with TemporaryDirectory() as d:
            tmp = build(Path(d), "| JOIN-001 | The client MUST discard it. | |\n")
            self.assertIn("SPEC-034", rules(run(tmp), "error"))

    def test_why_cell_is_captured(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | An older Welcome would roll the member back. |\n",
            )
            checker = run(tmp)
            self.assertIn("roll the member back", checker.requirements["JOIN-001"].why)

    def test_empty_why_cell_is_allowed(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            checker = run(tmp)
            self.assertEqual(rules(checker, "error"), [])
            self.assertEqual(checker.requirements["JOIN-001"].why, "")

    def test_escaped_pipe_stays_in_its_cell(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST write `a \\| b` as shown. | |\n",
            )
            checker = run(tmp)
            self.assertEqual(rules(checker, "error"), [])
            self.assertIn("a | b", checker.requirements["JOIN-001"].text)

    def test_bold_id_is_reported(self):
        """A bold identifier is the old form; it must not vanish from the checks."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| **JOIN-001** | Stale welcome | The client MUST discard it. | |\n",
            )
            checker = run(tmp)
            self.assertIn("SPEC-034", rules(checker, "error"))
            self.assertNotIn("JOIN-001", checker.requirements)

    def test_wrong_cell_count_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. |\n",
            )
            checker = run(tmp)
            self.assertIn("SPEC-034", rules(checker, "error"))
            self.assertNotIn("JOIN-001", checker.requirements)

    def test_row_outside_a_requirements_table_is_an_error(self):
        """A requirement pasted into another table is still parsed, and reported."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "\n| Term | Meaning | Note | More |\n| --- | --- | --- | --- |\n"
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            checker = run(tmp)
            self.assertIn("SPEC-034", rules(checker, "error"))
            self.assertIn("JOIN-001", checker.requirements)

    def test_table_ends_at_a_blank_line(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n\n"
                "Ordinary prose that mentions SHALL as a word.\n",
            )
            checker = run(tmp)
            self.assertEqual(
                checker.requirements["JOIN-001"].text, "The client MUST discard it."
            )
            self.assertEqual(rules(checker, "error"), [])


class Structure(unittest.TestCase):
    def test_unregistered_prefix_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                registry="# Prefix registry\n\n| Prefix | Spec | File |\n| --- | --- | --- |\n",
            )
            self.assertIn("SPEC-031", rules(run(tmp), "error"))

    def test_missing_terms_section_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "docs" / "specs").mkdir(parents=True)
            (tmp / "docs" / "specs" / "PREFIXES.md").write_text(REGISTRY)
            (tmp / "docs" / "specs" / "JOIN-joining-groups.md").write_text(
                "---\nprefix: JOIN\nstatus: draft\n---\n# T\n\nSummary.\n\n"
                "## Scope\n\nIn and out.\n\n## 1. Welcomes\n\nProse.\n"
            )
            self.assertIn("SPEC-003", rules(run(tmp), "error"))

    def test_out_of_order_sections_are_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra="\n## 3. Later\n\nProse.\n",
            )
            self.assertIn("SPEC-003", rules(run(tmp), "error"))

    def test_ceiling_is_enforced(self):
        with TemporaryDirectory() as d:
            body = "".join(
                f"| JOIN-{n:03d} | Rule number {n} | The client MUST do it. | |\n"
                for n in range(1, check.MAX_REQUIREMENTS + 2)
            )
            tmp = build(Path(d), body)
            self.assertIn("SPEC-005", rules(run(tmp), "error"))

    def test_bad_frontmatter_key_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "docs" / "specs").mkdir(parents=True)
            (tmp / "docs" / "specs" / "PREFIXES.md").write_text(REGISTRY)
            (tmp / "docs" / "specs" / "JOIN-joining-groups.md").write_text(
                "---\nprefix: JOIN\nstatus: draft\nowner: someone\n---\n# T\n\nS.\n\n"
                "## Scope\n\nx\n\n## Terms\n\nx\n\n## 1. A\n\nx\n"
            )
            self.assertIn("SPEC-002", rules(run(tmp), "error"))

    def test_requirement_in_a_fence_is_not_parsed(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                textwrap.dedent("""\
                | JOIN-001 | Stale welcome | The client MUST discard it. | |

                ```markdown
                | JOIN-999 | Example only | The client MUST do nothing. | |
                ```
                """),
            )
            checker = run(tmp)
            self.assertIn("JOIN-001", checker.requirements)
            self.assertNotIn("JOIN-999", checker.requirements)


class Links(unittest.TestCase):
    def code(self, tmp: Path, body: str, name: str = "src/lib.rs") -> None:
        path = tmp / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)

    def test_verifies_link_is_collected(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, "// verifies: JOIN-001\nfn test_welcome() {}\n")
            checker = run(tmp)
            self.assertEqual(len(checker.requirements["JOIN-001"].verifies), 1)
            self.assertEqual(rules(checker, "error"), [])

    def test_many_verifies_links_are_allowed(self):
        """Evidence at several boundaries is not capped."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(
                tmp,
                "".join(f"// verifies: JOIN-001\nfn t{n}() {{}}\n" for n in range(5)),
            )
            checker = run(tmp)
            self.assertEqual(len(checker.requirements["JOIN-001"].verifies), 5)
            self.assertEqual(rules(checker, "error"), [])

    def test_many_implements_links_are_allowed(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(
                tmp,
                "".join(f"// implements: JOIN-001\nfn f{n}() {{}}\n" for n in range(4))
                + "// verifies: JOIN-001\nfn t() {}\n",
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_stray_mention_of_approved_id_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, "// see JOIN-001 for the rule\nfn f() {}\n")
            self.assertIn("SPEC-053", rules(run(tmp), "error"))

    def test_stray_legacy_mention_is_a_warning(self):
        """Legacy ids are cleaned up with their spec, so they must not block a PR."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, "// CFG-051: the identifier is the binding\nfn f() {}\n")
            checker = run(tmp)
            self.assertIn("SPEC-053", rules(checker, "warning"))
            self.assertEqual(rules(checker, "error"), [])

    def test_retired_section_registers_prefixes_like_legacy(self):
        """A renamed section must not unregister its prefixes silently.

        An unregistered prefix is skipped outright, so a stale mention stops
        being reported rather than being cleaned up.
        """
        registry = REGISTRY.replace("## Legacy", "## Retired")
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                registry=registry,
            )
            self.code(tmp, "// CFG-051: the identifier is the binding\nfn f() {}\n")
            checker = run(tmp)
            self.assertIn("SPEC-053", rules(checker, "warning"))
            self.assertEqual(rules(checker, "error"), [])

    def test_unknown_id_with_known_prefix_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, "// verifies: JOIN-404\nfn f() {}\n")
            self.assertIn("SPEC-053", rules(run(tmp), "error"))

    def test_multiple_ids_on_one_token(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                textwrap.dedent("""\
                | JOIN-001 | Stale welcome | The client MUST discard it. | |
                | JOIN-002 | Fresh welcome | The client MUST accept it. | |
                """),
            )
            self.code(tmp, "// verifies: JOIN-001, JOIN-002\nfn f() {}\n")
            checker = run(tmp)
            self.assertEqual(rules(checker, "error"), [])
            self.assertEqual(len(checker.requirements["JOIN-002"].verifies), 1)

    def test_gate_turns_missing_evidence_into_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.assertIn("SPEC-051", rules(run(tmp, gate="warn"), "warning"))
            self.assertIn("SPEC-051", rules(run(tmp, gate="error"), "error"))

    def test_draft_requirement_needs_no_evidence(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                status="draft",
            )
            checker = run(tmp, gate="error")
            self.assertNotIn("SPEC-051", rules(checker, "error"))

    def test_waiver_satisfies_the_gate(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nkind = "analysis"\nreason = "reviewed by hand"\n'
            )
            self.assertNotIn("SPEC-051", rules(run(tmp, gate="error"), "error"))

    def test_waiver_without_a_reason_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nkind = "analysis"\n'
            )
            self.assertIn("SPEC-055", rules(run(tmp), "error"))

    def test_waiver_for_unknown_id_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-777"\nkind = "analysis"\nreason = "typo"\n'
            )
            self.assertIn("SPEC-055", rules(run(tmp), "error"))

    def test_stale_waiver_is_a_warning(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nkind = "analysis"\nreason = "reviewed by hand"\n'
            )
            self.code(tmp, "// verifies: JOIN-001\nfn f() {}\n")
            self.assertIn("SPEC-055", rules(run(tmp), "warning"))

    def test_sdk_languages_are_scanned(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(
                tmp,
                "// verifies: JOIN-001\nfunc testWelcome() {}\n",
                name="sdks/ios/Tests/WelcomeTests.swift",
            )
            self.assertEqual(len(run(tmp).requirements["JOIN-001"].verifies), 1)

    def test_skipped_directories_are_not_scanned(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(
                tmp, "// verifies: JOIN-001\nfn f() {}\n", name="target/debug/gen.rs"
            )
            self.assertEqual(run(tmp).requirements["JOIN-001"].verifies, [])


class CrossReferences(unittest.TestCase):
    def test_reference_to_unknown_id_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it per API-900. | |\n",
            )
            self.assertIn("SPEC-042", rules(run(tmp), "error"))

    def test_backticks_do_not_exempt_a_reference(self):
        """A code span is still a reference; only listed illustrations are exempt."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST follow `API-900`. | |\n",
            )
            self.assertIn("SPEC-042", rules(run(tmp), "error"))

    def test_listed_illustration_is_allowed(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | An id looks like `PREFIX-NNN`; the client MUST parse it. | |\n",
            )
            self.assertNotIn("SPEC-042", rules(run(tmp), "error"))

    def test_unregistered_prefix_reference_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST follow XYZ-900. | |\n",
            )
            self.assertIn("SPEC-042", rules(run(tmp), "error"))

    def test_hyphenated_non_identifier_is_not_a_reference(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST use SHA-256. | |\n",
            )
            self.assertNotIn("SPEC-042", rules(run(tmp), "error"))


class Proto(unittest.TestCase):
    def block(self, proto: str) -> str:
        return f"\n```proto\n{proto}\n```\n"

    def test_valid_wire_message_passes(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "message WelcomeMessage {\n"
                    "  bytes installation_key = 1;  // 32 bytes\n"
                    "  bytes data = 2;\n"
                    "}"
                ),
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_oneof_is_allowed_in_proto(self):
        """The shape WebIDL cannot express is the normal case on the wire."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "message WelcomeMessage {\n"
                    "  oneof version {\n"
                    "    V1 v1 = 1;\n"
                    "    WelcomePointer welcome_pointer = 2;\n"
                    "  }\n"
                    "}"
                ),
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_field_without_a_number_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block("message Welcome {\n  bytes data;\n}"),
            )
            self.assertIn("SPEC-047", rules(run(tmp), "error"))

    def test_unbalanced_proto_braces_are_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block("message Welcome {\n  bytes data = 1;"),
            )
            self.assertIn("SPEC-047", rules(run(tmp), "error"))

    def test_webidl_rules_do_not_apply_to_proto(self):
        """`bytes` is correct in proto; sequence<octet> is a WebIDL-only rule."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block("message Welcome {\n  bytes data = 1;\n}"),
            )
            checker = run(tmp)
            self.assertNotIn("SPEC-070", rules(checker, "error"))
            self.assertEqual(rules(checker, "error"), [])


class WebIDL(unittest.TestCase):
    def block(self, idl: str) -> str:
        return f"\n```webidl\n{idl}\n```\n"

    def test_valid_block_passes(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "dictionary Welcome {\n"
                    "  required sequence<octet> installation_key;   // 32 bytes\n"
                    "};"
                ),
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_interface_is_rejected(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block("interface Welcome {\n  void send();\n};"),
            )
            self.assertIn("SPEC-049", rules(run(tmp), "error"))

    def test_union_of_dictionaries_is_rejected(self):
        """WebIDL cannot distinguish two dictionary types in a union."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "dictionary WelcomeMessage { required sequence<octet> data; };\n"
                    "dictionary WelcomePointer { required sequence<octet> pointer; };\n"
                    "typedef (WelcomeMessage or WelcomePointer) Welcome;"
                ),
            )
            self.assertIn("SPEC-071", rules(run(tmp), "error"))

    def test_binding_byte_types_are_rejected(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "dictionary Welcome {\n  required Uint8Array data;\n};"
                ),
            )
            self.assertIn("SPEC-070", rules(run(tmp), "error"))

    def test_unbalanced_braces_are_rejected(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "dictionary Welcome {\n  required sequence<octet> data;"
                ),
            )
            self.assertIn("SPEC-049", rules(run(tmp), "error"))

    def test_comment_mentioning_a_type_is_ignored(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.block(
                    "dictionary Welcome {\n"
                    "  required sequence<octet> data;   // not a Uint8Array\n"
                    "};"
                ),
            )
            self.assertNotIn("SPEC-070", rules(run(tmp), "error"))


if __name__ == "__main__":
    unittest.main()


class ReviewRegressions(unittest.TestCase):
    """Defects found by adversarial review on 2026-09-17. Each reproduced first."""

    def code(self, tmp: Path, body: str, name: str = "src/lib.rs") -> None:
        path = tmp / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)

    def test_keyword_anywhere_in_the_cell_is_seen(self):
        """The whole Requirement cell is the obligation."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Wrapped rule | When a Welcome arrives late, the client MUST discard it. | |\n",
            )
            checker = run(tmp)
            self.assertIn("MUST discard it", checker.requirements["JOIN-001"].text)
            self.assertNotIn("SPEC-035", rules(checker, "error"))

    def test_shall_late_in_the_cell_is_caught(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Wrapped rule | When a Welcome arrives late, the client SHALL discard it. | |\n",
            )
            self.assertIn("SPEC-035", rules(run(tmp), "error"))

    def test_malformed_id_is_reported_not_skipped(self):
        """A two-digit id must not vanish from every check."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d), "| JOIN-01 | Bad id | The client MUST discard it. | |\n"
            )
            checker = run(tmp)
            self.assertEqual(checker.requirements, {})
            self.assertIn("SPEC-034", rules(checker, "error"))

    def test_four_digit_id_is_rejected(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d), "| JOIN-1000 | Wide id | The client MUST discard it. | |\n"
            )
            checker = run(tmp)
            self.assertEqual(checker.requirements, {})
            self.assertIn("SPEC-034", rules(checker, "error"))

    def test_string_literal_is_not_evidence(self):
        """A link token only counts inside a comment."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, 'const NOTE: &str = "verifies: JOIN-001";\n')
            checker = run(tmp)
            self.assertEqual(checker.requirements["JOIN-001"].verifies, [])

    def test_comment_forms_are_recognized(self):
        for leader, name in (
            ("//", "src/a.rs"),
            ("#", "src/b.py"),
            ("--", "src/c.sql"),
        ):
            with TemporaryDirectory() as d:
                tmp = build(
                    Path(d),
                    "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                )
                self.code(tmp, f"{leader} verifies: JOIN-001\nx\n", name=name)
                self.assertEqual(
                    len(run(tmp).requirements["JOIN-001"].verifies),
                    1,
                    f"{leader} comment not recognized",
                )

    def test_duplicate_proto_field_number_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=(
                    "\n```proto\n"
                    "message Welcome {\n"
                    "  bytes data = 1;\n"
                    "  bytes other = 1;\n"
                    "}\n"
                    "```\n"
                ),
            )
            self.assertIn("SPEC-047", rules(run(tmp), "error"))

    def test_valid_typedef_is_not_reported_unterminated(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra="\n```webidl\ntypedef unsigned long Counter;\n```\n",
            )
            self.assertEqual(rules(run(tmp), "error"), [])

    def test_reused_prefix_below_floor_is_rejected(self):
        """A number that already meant a legacy requirement cannot be reallocated."""
        registry = REGISTRY.replace(
            "| `API` | Backend API | `docs/specs/API-backend-api.md` |",
            "| `API` | Backend API | `docs/specs/API-a.md` |",
        )
        with TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "docs" / "specs").mkdir(parents=True)
            (tmp / "docs" / "specs" / "PREFIXES.md").write_text(registry)
            (tmp / "docs" / "specs" / "API-a.md").write_text(
                "---\nprefix: API\nstatus: draft\n---\n# A\n\nS.\n\n"
                "## Scope\n\nx\n\n## Terms\n\nx\n\n## 1. A\n\nProse.\n\n"
                "| ID | Title | Requirement | Why |\n| --- | --- | --- | --- |\n"
                "| API-001 | Topic derivation | The backend MUST derive it. | |\n"
            )
            self.assertIn("SPEC-032", rules(run(tmp), "error"))

    def test_reused_prefix_above_floor_is_allowed(self):
        registry = REGISTRY.replace(
            "| `API` | Backend API | `docs/specs/API-backend-api.md` |",
            "| `API` | Backend API | `docs/specs/API-a.md` |",
        )
        with TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "docs" / "specs").mkdir(parents=True)
            (tmp / "docs" / "specs" / "PREFIXES.md").write_text(registry)
            (tmp / "docs" / "specs" / "API-a.md").write_text(
                "---\nprefix: API\nstatus: draft\n---\n# A\n\nS.\n\n"
                "## Scope\n\nx\n\n## Terms\n\nx\n\n## 1. A\n\nProse.\n\n"
                "| ID | Title | Requirement | Why |\n| --- | --- | --- | --- |\n"
                "| API-200 | Topic derivation | The backend MUST derive it. | |\n"
            )
            self.assertNotIn("SPEC-032", rules(run(tmp), "error"))

    def test_waiver_without_a_kind_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nreason = "later"\n'
            )
            self.assertIn("SPEC-057", rules(run(tmp), "error"))

    def test_gap_waiver_needs_an_owner_and_issue(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nkind = "gap"\nreason = "not built"\n'
            )
            self.assertIn("SPEC-057", rules(run(tmp), "error"))

    def test_complete_gap_waiver_passes(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nkind = "gap"\nreason = "not built"\n'
                'owner = "backend"\nissue = "https://example.invalid/1"\n'
            )
            self.assertNotIn("SPEC-057", rules(run(tmp), "error"))


class SecondReviewRegressions(unittest.TestCase):
    """Defects found by the second adversarial review. Each reproduced first."""

    def code(self, tmp: Path, body: str, name: str = "src/lib.rs") -> None:
        path = tmp / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)

    def test_raw_string_is_not_evidence(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, 'const DOC: &str = r#"\n// verifies: JOIN-001\n"#;\n')
            self.assertEqual(run(tmp).requirements["JOIN-001"].verifies, [])

    def test_should_actor_must_precede_the_keyword(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client SHOULD notify the app. | |\n",
            )
            self.assertIn("SPEC-035", rules(run(tmp), "error"))

    def test_should_mixed_with_must_is_rejected(self):
        """A MUST must not inherit the SHOULD evidence exemption."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                (
                    "| JOIN-001 | Stale welcome | When an app SHOULD retry, "
                    "the client MUST discard it. | |\n"
                ),
            )
            self.assertIn("SPEC-035", rules(run(tmp), "error"))

    def test_malformed_link_token_is_not_truncated(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            self.code(tmp, "// verifies: JOIN-0010\nfn t() {}\n")
            self.assertEqual(run(tmp).requirements["JOIN-001"].verifies, [])

    def test_example_id_is_a_reference_outside_the_format_docs(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST comply with JOIN-047. | |\n",
            )
            self.assertIn("SPEC-042", rules(run(tmp), "error"))

    def proto(self, body: str) -> str:
        return f"\n```proto\n{body}\n```\n"

    def test_oneof_shares_the_parent_number_space(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.proto(
                    "message Sample {\n  bytes id = 1;\n"
                    "  oneof payload {\n    bytes data = 1;\n  }\n}"
                ),
            )
            self.assertIn("SPEC-047", rules(run(tmp), "error"))

    def test_enum_alias_is_allowed(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.proto(
                    "enum State {\n  option allow_alias = true;\n"
                    "  UNKNOWN = 0;\n  STARTED = 1;\n  RUNNING = 1;\n}"
                ),
            )
            self.assertNotIn("SPEC-047", rules(run(tmp), "error"))

    def test_nested_message_has_its_own_number_space(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra=self.proto(
                    "message Outer {\n  message Inner {\n    bytes a = 1;\n  }\n"
                    "  bytes b = 1;\n}"
                ),
            )
            self.assertNotIn("SPEC-047", rules(run(tmp), "error"))

    def test_gap_waiver_survives_a_link(self):
        """Evidence for one path does not close a recorded implementation gap."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
            )
            (tmp / "docs" / "specs" / "waivers.toml").write_text(
                '[[waiver]]\nid = "JOIN-001"\nkind = "gap"\nreason = "restart broken"\n'
                'owner = "backend"\nissue = "https://example.invalid/1"\n'
            )
            self.code(tmp, "// verifies: JOIN-001\nfn t() {}\n")
            self.assertEqual(
                [
                    f
                    for f in run(tmp).findings
                    if f.rule == "SPEC-055" and "remove" in f.message
                ],
                [],
            )

    def test_moved_id_needs_owns(self):
        registry = REGISTRY + "| `GMOD` | Modifying groups | `docs/specs/GMOD-g.md` |\n"
        for owns, expect in ((None, True), ("JOIN-001", False)):
            with TemporaryDirectory() as d:
                tmp = Path(d)
                (tmp / "docs" / "specs").mkdir(parents=True)
                (tmp / "docs" / "specs" / "PREFIXES.md").write_text(registry)
                head = "---\nprefix: GMOD\nstatus: draft\n"
                if owns:
                    head += f"owns: {owns}\n"
                head += "---\n# G\n\nS.\n\n## Scope\n\nx\n\n## Terms\n\nx\n\n## 1. A\n\nProse.\n\n"
                head += (
                    "| ID | Title | Requirement | Why |\n| --- | --- | --- | --- |\n"
                )
                (tmp / "docs" / "specs" / "GMOD-g.md").write_text(
                    head + "| JOIN-001 | Moved rule | The client MUST reject it. | |\n"
                )
                self.assertEqual("SPEC-031" in rules(run(tmp), "error"), expect)

    def pending(self, join_status: str, ident_status: str) -> list[tuple[str, str]]:
        registry = (
            "# Prefix registry\n\n## Active\n\n| Prefix | Spec | File |\n| --- | --- | --- |\n"
            "| `JOIN` | Joining groups | `docs/specs/JOIN-j.md` |\n"
            "| `IDENT` | Identity updates | `docs/specs/IDENT-i.md` |\n"
        )
        head = (
            "---\nprefix: {p}\nstatus: {st}\n---\n# T\n\nS.\n\n"
            "## Scope\n\nx\n\n## Terms\n\nx\n\n## 1. A\n\nProse.\n\n"
            "| ID | Title | Requirement | Why |\n| --- | --- | --- | --- |\n"
        )
        with TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "docs" / "specs").mkdir(parents=True)
            (tmp / "docs" / "specs" / "PREFIXES.md").write_text(registry)
            (tmp / "docs" / "specs" / "JOIN-j.md").write_text(
                head.format(p="JOIN", st=join_status)
                + "| JOIN-001 | Proof check | The client MUST verify it as ?IDENT requires. | |\n"
            )
            (tmp / "docs" / "specs" / "IDENT-i.md").write_text(
                head.format(p="IDENT", st=ident_status)
                + "| IDENT-001 | Proof rule | The client MUST check it. | |\n"
            )
            checker = check.Checker(tmp, gate="warn")
            checker.run()
            return [
                (f.level, f.rule)
                for f in checker.findings
                if f.rule in ("SPEC-078", "SPEC-079")
            ]

    def test_pending_ref_is_a_warning_while_both_are_drafts(self):
        self.assertEqual(self.pending("draft", "draft"), [("warning", "SPEC-078")])

    def test_pending_ref_blocks_approving_the_referring_spec(self):
        self.assertIn(("error", "SPEC-079"), self.pending("approved", "draft"))

    def test_pending_ref_becomes_due_when_the_owner_is_approved(self):
        self.assertIn(("error", "SPEC-079"), self.pending("draft", "approved"))

    def test_pending_ref_to_unregistered_prefix_is_an_error(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Proof check | The client MUST verify it as ?NOPE requires. | |\n",
            )
            self.assertIn("SPEC-078", rules(run(tmp), "error"))

    def test_second_actor_with_its_own_must_warns(self):
        """SPEC-034 is unenforceable exactly; this is the shape that signals two."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                (
                    "| JOIN-001 | Stale welcome | The client MUST discard it, "
                    "and the backend MUST reject it. | |\n"
                ),
            )
            checker = run(tmp)
            self.assertIn("SPEC-034", rules(checker, "warning"))
            self.assertNotIn("SPEC-034", rules(checker, "error"))

    def test_one_actor_with_clauses_is_quiet(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                (
                    "| JOIN-001 | Stale welcome | The client MUST discard it and "
                    "MUST NOT report it. | |\n"
                ),
            )
            self.assertNotIn("SPEC-034", rules(run(tmp), "warning"))

    def test_condition_with_and_is_quiet(self):
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                (
                    "| JOIN-001 | Stale welcome | When a Welcome is late and "
                    "unreadable, the client MUST discard it. | |\n"
                ),
            )
            self.assertNotIn("SPEC-034", rules(run(tmp), "warning"))

    def test_value_table_needs_no_wrapper_requirement(self):
        """SPEC-087: a table of owned values binds without a pointing requirement."""
        with TemporaryDirectory() as d:
            tmp = build(
                Path(d),
                "| JOIN-001 | Stale welcome | The client MUST discard it. | |\n",
                extra="\n| Extension | Identifier |\n| --- | --- |\n| Wrapper | `0xff03` |\n",
            )
            self.assertEqual(rules(run(tmp), "error"), [])
