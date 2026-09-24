#!/usr/bin/env python3
"""Inventory the public surface of the four current SDK packages.

This is a source inventory, not a compiler ABI dump. Generated Swift sources are
counted by source family; the manifest keeps one disjoint group row per family.
Run from the repository root: python3 dev/sdk/inventory.py [--write].
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True  # Keep source inventory runs from leaving dev/sdk/__pycache__.
SWIFT = ROOT / "sdks/ios/Sources/XMTPiOS"
KOTLIN = ROOT / "sdks/android/library/src/main/java/org/xmtp/android/library"
TS_ROOTS = {
    "Node": ROOT / "sdks/node/src",
    "Browser": ROOT / "sdks/browser/src",
}
OUT = ROOT / "docs/self-hosted/sdk-api-manifest.md"


@dataclass(frozen=True)
class Entry:
    sdk: str
    source: str
    line: int
    name: str
    kind: str
    count: int = 1

    @property
    def key(self) -> str:
        return f"{self.source}:{self.line} {self.name}"

    @property
    def display_name(self) -> str:
        if self.sdk == "Kotlin" and self.name == "Client.Companion.register":
            return "Client.Companion.register(codec)"
        if self.name.startswith("func "):
            return self.name
        if self.kind in {"function", "free function"}:
            return f"func {self.name}"
        if self.sdk in {"Node", "Browser"} and (self.name in {"encryptAttachment", "decryptAttachment", "flushTelemetry", "initLogging"}
                or re.match(r"^(?:encode|contentType)[A-Z]", self.name)):
            return f"func {self.name}"
        return self.name


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def compact_name(line: str, language: str) -> tuple[str, str] | None:
    if language == "Swift":
        match = re.search(
            r"\b(typealias|class|struct|enum|protocol|actor|func|var|let|init|subscript|operator|extension)\b\s*([A-Za-z_][\w]*|[+*/=<>!-]+)?",
            line,
        )
    else:
        match = re.search(
            r"\b(typealias|class|interface|object|fun|val|var|constructor)\b\s*([A-Za-z_][\w]*)?",
            line,
        )
    if not match:
        return None
    kind, name = match.groups()
    return name or kind, kind


def swift_scan(text: str, source: str) -> list[Entry]:
    """Read source declarations, including public members and SPI declarations."""
    entries: list[Entry] = []
    depth = 0
    contexts: list[tuple[int, str, str]] = []
    pending_type: tuple[str, str] | None = None
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith(("//", "*")):
            continue
        while contexts and depth < contexts[-1][0]:
            contexts.pop()
        owner = contexts[-1][1] if contexts else ""
        at_surface = not contexts or depth == contexts[-1][0]
        explicit = re.match(r"^(?:@[_\w.]+(?:\([^)]*\))?\s+)*(?:public|open)\s+", stripped)
        parsed = compact_name(stripped, "Swift") if explicit else None
        scoped_extension = re.match(r"^extension\s+([A-Za-z_]\w*)\b", stripped) if not explicit else None
        if explicit and parsed and at_surface:
            name, kind = parsed
            if kind == "extension":
                if "{" in line:
                    contexts.append((depth + 1, name, kind))
                else:
                    pending_type = (name, kind)
            else:
                symbol = f"{owner}.{name}" if owner else name
                if kind == "func" and not owner:
                    symbol = f"func {name}"
                elif kind in {"var", "let"} and not owner:
                    symbol = f"{kind} {name}"
                entries.append(Entry("Swift", source, number, symbol, kind))
                if kind in {"class", "struct", "enum", "protocol", "actor"}:
                    if "{" in line:
                        contexts.append((depth + 1, symbol, kind))
                    else:
                        pending_type = (symbol, kind)
        elif scoped_extension and at_surface:
            extension_scope = (scoped_extension.group(1), "scope")
            if "{" in line:
                contexts.append((depth + 1, *extension_scope))
            else:
                pending_type = extension_scope
        elif contexts and at_surface:
            inherited_kind = contexts[-1][2]
            if inherited_kind in {"extension", "protocol"}:
                member = compact_name(stripped, "Swift")
                if member and not stripped.startswith(("private ", "internal ", "fileprivate ")):
                    name, kind = member
                    if kind != "extension":
                        entries.append(Entry("Swift", source, number, f"{owner}.{name}", kind))
            elif inherited_kind == "enum" and stripped.startswith("case "):
                for case in re.split(r",\s*(?![^()]*\))", stripped[5:].split("//")[0]):
                    name = re.match(r"[A-Za-z_]\w*", case.strip())
                    if name:
                        entries.append(Entry("Swift", source, number, f"{owner}.{name.group()}", "case"))
        code = line.split("//", 1)[0]
        if pending_type and "{" in code:
            contexts.append((depth + 1, *pending_type))
            pending_type = None
        depth += code.count("{") - code.count("}")
    return entries


def swift_inventory() -> list[Entry]:
    entries: list[Entry] = []
    generated: list[Entry] = []
    proto_count = 0
    for path in sorted(SWIFT.rglob("*.swift")):
        scanned = swift_scan(path.read_text(), rel(path))
        relative = path.relative_to(SWIFT).as_posix()
        if relative.startswith("Proto/"):
            proto_count += len(scanned)
        elif relative == "Libxmtp/xmtpv3.swift":
            generated = scanned
        else:
            entries.extend(scanned)
    entries.append(Entry("Swift", "sdks/ios/Sources/XMTPiOS/Proto/*.pb.swift", 0,
                         "pattern: Proto/*.pb.swift public declarations", "generated family", proto_count))
    # A type used in a public signature is individually listed with its
    # members. The remaining old bridge output has disjoint family rules.
    directly_used: set[str] = set()
    source_lines: dict[str, list[str]] = {}
    for entry in entries:
        if entry.kind == "generated family":
            continue
        if entry.source not in source_lines:
            source_lines[entry.source] = (ROOT / entry.source).read_text().splitlines()
        lines = source_lines[entry.source]
        signature: list[str] = []
        for line in lines[entry.line - 1:entry.line + 11]:
            signature.append(line)
            if "{" in line or "=" in line or ";" in line:
                break
        directly_used.update(re.findall(r"\b(?:Ffi[A-Za-z0-9_]+|XmtpApiClient|DbOptions)\b", "\n".join(signature)))
    patterns = [
        ("FfiConverter internals", re.compile(r"^(?:FfiConverter[^.]*|func FfiConverter[^ ]*)(?:\..*)?$")),
        ("callback protocols and implementations", re.compile(r"^Ffi(?!Converter)[A-Za-z0-9_]*(?:Callback|Listener)(?:Impl)?(?:\..*)?$")),
        ("other Ffi binding types and members", re.compile(r"^Ffi[A-Za-z0-9_]+(?:\..*)?$")),
        ("internal free functions", re.compile(r"^func .+$")),
        ("other generated declarations", re.compile(r"^.+$")),
    ]
    buckets: dict[str, list[Entry]] = {label: [] for label, _ in patterns}
    for entry in generated:
        root = entry.name.split(".")[0]
        if root in directly_used:
            entries.append(entry)
            continue
        for label, pattern in patterns:
            if pattern.fullmatch(entry.name):
                buckets[label].append(entry)
                break
    for label, pattern in patterns:
        count = len(buckets[label])
        if count:
            entries.append(Entry("Swift", "sdks/ios/Sources/XMTPiOS/Libxmtp/xmtpv3.swift", 0,
                                 f"pattern: {pattern.pattern} [after prior family rules; excluding public-signature Ffi roots]",
                                 "generated family", count))
    return entries


KOTLIN_DECL = re.compile(
    r"\b(typealias|class|interface|object|fun|val|var|constructor)\b\s*(?:<[^>]+>\s*)?([A-Za-z_][\w]*)?"
)


def kotlin_scan(text: str, source: str) -> list[Entry]:
    entries: list[Entry] = []
    depth = 0
    contexts: list[tuple[int, str, bool, str]] = []
    pending: tuple[str, bool, str] | None = None
    class_header = False
    header_parens = 0
    in_block_comment = False
    for number, line in enumerate(text.splitlines(), 1):
            code = line
            if in_block_comment:
                if "*/" not in code:
                    continue
                code = code.split("*/", 1)[1]
                in_block_comment = False
            if "/*" in code:
                before, after = code.split("/*", 1)
                if "*/" in after:
                    code = before + after.split("*/", 1)[1]
                else:
                    code = before
                    in_block_comment = True
            code = code.split("//", 1)[0]
            stripped = code.strip()
            while contexts and depth < contexts[-1][0]:
                contexts.pop()
            owner_public = not contexts or contexts[-1][2]
            owner = contexts[-1][3] if contexts else ""
            at_surface = (depth == 0) if not contexts else (contexts[-1][1] in {"type", "enum"} and depth == contexts[-1][0])
            match = KOTLIN_DECL.search(stripped) if stripped and not stripped.startswith("@") else None
            # Visibility belongs to the declaration. A private constructor
            # does not make its enclosing class private.
            hidden = bool(match and re.search(r"\b(private|internal|protected)\b", stripped[:match.start()]))
            if match and owner_public and not hidden and (at_surface or class_header) and (not class_header or (pending is not None and pending[1])):
                kind, name = match.groups()
                name = name or ("Companion" if kind == "object" and "companion" in stripped else kind)
                if kind == "fun":
                    function = re.search(r"\bfun\s+(?:<[^>]+>\s*)?([\w.]+)\s*\(", stripped)
                    if function:
                        name = function.group(1)
                if kind in {"val", "var"}:
                    prop = re.search(r"\b(?:val|var)\s+([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)", stripped)
                    if prop:
                        name = prop.group(1)
                if class_header and kind in {"val", "var"}:
                    kind = "constructor property"
                    owner = pending[2] if pending else owner
                if kind == "constructor":
                    name = "init"
                symbol = f"{owner}.{name}" if owner else name
                if kind == "fun" and not owner and "." not in name:
                    symbol = f"func {name}"
                elif kind in {"val", "var"} and not owner and "." not in name:
                    symbol = f"{kind} {name}"
                entries.append(Entry("Kotlin", source, number, symbol, kind))
                if kind == "class":
                    for prop in re.finditer(r"\b(?:val|var)\s+([A-Za-z_]\w*)", stripped[match.end():]):
                        entries.append(Entry("Kotlin", source, number, f"{symbol}.{prop.group(1)}", "constructor property"))
            elif contexts and contexts[-1][1] == "enum" and depth == contexts[-1][0] and owner_public:
                variant = re.match(r"([A-Za-z_]\w*)\s*(?:,|;|\((?!this\b))", stripped)
                if variant:
                    entries.append(Entry("Kotlin", source, number, f"{owner}.{variant.group(1)}", "enum case"))
            if at_surface and match and match.group(1) in {"class", "interface", "object"}:
                name = match.group(2) or ("Companion" if "companion" in stripped else match.group(1))
                symbol = f"{owner}.{name}" if owner else name
                pending = ("enum" if "enum class" in stripped else "type", owner_public and not hidden, symbol)
                header_parens = code.count("(") - code.count(")")
                class_header = header_parens > 0
            elif at_surface and match and match.group(1) in {"fun", "constructor"}:
                pending = ("body", owner_public and not hidden, owner)
                class_header = False
            elif class_header:
                header_parens += code.count("(") - code.count(")")
                class_header = header_parens > 0
            opens = code.count("{")
            closes = code.count("}")
            if opens:
                if pending:
                    contexts.append((depth + 1, *pending))
                    pending = None
                elif not at_surface:
                    contexts.append((depth + 1, "body", owner_public, owner))
            depth += opens - closes
            if depth < 0:
                depth = 0
    return entries


def kotlin_inventory() -> list[Entry]:
    return [entry for path in sorted(KOTLIN.rglob("*.kt")) for entry in kotlin_scan(path.read_text(), rel(path))]


def ts_resolve(root: Path, spec: str) -> Path | None:
    if not spec.startswith("."):
        return None
    base = (root / spec).resolve()
    for candidate in (base.with_suffix(".ts"), base / "index.ts"):
        if candidate.is_file():
            return candidate
    return None


def ts_exports(path: Path, seen: set[Path]) -> list[Entry]:
    if path in seen:
        return []
    seen.add(path)
    sdk = "Node" if "/node/" in path.as_posix() else "Browser"
    text = path.read_text()
    entries: list[Entry] = []
    statement = re.compile(r"export\s+(?:type\s+)?(?:\{([^}]+)\}|\*)\s+from\s+['\"]([^'\"]+)['\"]", re.S)
    for found in statement.finditer(text):
        names, spec = found.groups()
        target = ts_resolve(path.parent, spec)
        if names:
            for item in names.split(","):
                item = re.sub(r"/\*.*?\*/", "", item, flags=re.S).strip()
                item = re.sub(r"^type\s+", "", item)
                if not item:
                    continue
                name = item.split(" as ")[-1].strip()
                line = text.count("\n", 0, found.start()) + 1
                kind = "binding re-export" if not target else "re-export"
                if target and re.search(rf"export\s+(?:const|function)\s+{re.escape(name)}\b[\s\S]{{0,200}}?=>", target.read_text()):
                    kind = "free function"
                entries.append(Entry(sdk, rel(path) if target else spec, line, name, kind))
                if target and re.search(rf"export\s+(?:abstract\s+)?class\s+{re.escape(name)}\b", target.read_text()):
                    entries.extend(ts_class_members(target, name, sdk))
                elif target:
                    entries.extend(ts_object_members(target, name, sdk))
        elif target:
            entries.extend(ts_exports(target, seen))
    # Direct exports from modules reached by a wildcard.
    for number, line in enumerate(text.splitlines(), 1):
        found = re.match(r"^export\s+(?:(?:declare|abstract|default)\s+)*(type|interface|class|enum|function|const|let|var)\s+([A-Za-z_]\w*)", line)
        if found:
            kind, name = found.groups()
            if kind == "const" and re.search(r"=>", "\n".join(text.splitlines()[number - 1:number + 8])):
                kind = "free function"
            entries.append(Entry(sdk, rel(path), number, name, kind))
            if kind == "class":
                entries.extend(ts_class_members(path, name, sdk))
            elif kind in {"type", "interface"}:
                entries.extend(ts_object_members(path, name, sdk))
    return entries


def ts_object_members(path: Path, type_name: str, sdk: str) -> list[Entry]:
    """Inventory named fields of an exported object type or interface."""
    lines = path.read_text().splitlines()
    start = next((i for i, line in enumerate(lines) if re.match(
        rf"^export\s+(?:type|interface)\s+{re.escape(type_name)}\b", line)), None)
    if start is None:
        return []
    header_has_object = False
    for index in range(start, len(lines)):
        if index > start and re.match(r"^export\s+", lines[index]):
            break
        if "{" in lines[index]:
            header_has_object = True
            break
        if ";" in lines[index]:
            break
    if not header_has_object:
        return []
    entries: list[Entry] = []
    depth = 0
    entered = False
    block_comment = False
    owners: list[tuple[int, str]] = []
    for index in range(start, len(lines)):
        line = lines[index]
        code = line
        if block_comment:
            if "*/" not in code:
                continue
            code = code.split("*/", 1)[1]
            block_comment = False
        if "/*" in code:
            before, after = code.split("/*", 1)
            if "*/" in after:
                code = before + after.split("*/", 1)[1]
            else:
                code = before
                block_comment = True
        code = code.split("//", 1)[0]
        while owners and depth < owners[-1][0]:
            owners.pop()
        if entered and depth >= 1:
            match = re.match(r"\s*([A-Za-z_]\w*)\??\s*[:(]", code)
            if match:
                owner = owners[-1][1] if owners else type_name
                field = f"{owner}.{match.group(1)}"
                entries.append(Entry(sdk, rel(path), index + 1, field, "type member"))
                if "{" in code[match.end():]:
                    owners.append((depth + 1, field))
        elif not entered and "{" in code:
            inline = re.search(r"\{\s*([A-Za-z_]\w*)\??\s*:", code)
            if inline:
                entries.append(Entry(sdk, rel(path), index + 1, f"{type_name}.{inline.group(1)}", "type member"))
        opens = code.count("{")
        closes = code.count("}")
        if opens:
            entered = True
        depth += opens - closes
        if entered and depth <= 0:
            break
    return entries


def ts_class_members(path: Path, class_name: str, sdk: str) -> list[Entry]:
    lines = path.read_text().splitlines()
    start = next((i for i, line in enumerate(lines) if re.search(rf"\bclass\s+{re.escape(class_name)}\b", line)), None)
    if start is None:
        return []
    entries: list[Entry] = []
    depth = 0
    entered = False
    quote = ""
    block_comment = False
    pending_signature = False
    for index in range(start, len(lines)):
        line = lines[index]
        stripped = line.strip()
        if sdk == "Node" and class_name == "Conversation" and pending_signature and re.match(r"_client\s*:", stripped):
            entries.append(Entry(sdk, rel(path), index + 1, "Conversation._client", "constructor parameter"))
        if entered and depth == 1 and not pending_signature and stripped and not stripped.startswith(("//", "*", "#", "private ", "protected ")):
            if re.match(r"\[Symbol\.asyncIterator\]\s*\(", stripped):
                entries.append(Entry(sdk, rel(path), index + 1, f"{class_name}[Symbol.asyncIterator]", "member"))
            else:
                match = re.match(r"(?:(?:public|static|async|readonly|override|declare|get|set)\s+)*([A-Za-z_]\w*)\s*(?:[<(=:?]|$)", stripped)
                if match and match.group(1) not in {"return", "throw", "if", "for", "while"}:
                    name = match.group(1)
                    entries.append(Entry(sdk, rel(path), index + 1, f"{class_name}.{name}", "member"))
                    if "(" in stripped and "{" not in stripped and ";" not in stripped:
                        pending_signature = True
        braces: list[str] = []
        cursor = 0
        while cursor < len(line):
            char = line[cursor]
            following = line[cursor:cursor + 2]
            if block_comment:
                if following == "*/":
                    block_comment = False
                    cursor += 2
                    continue
            elif quote:
                if char == "\\":
                    cursor += 2
                    continue
                if char == quote:
                    quote = ""
            elif following == "//":
                break
            elif following == "/*":
                block_comment = True
                cursor += 2
                continue
            elif char in {"'", '"', "`"}:
                quote = char
            elif char in "{}":
                braces.append(char)
            cursor += 1
        depth += braces.count("{") - braces.count("}")
        if "{" in braces:
            entered = True
            pending_signature = False
        elif ";" in line and pending_signature:
            pending_signature = False
        if entered and depth <= 0:
            break
    return entries


def ts_inventory(sdk: str) -> list[Entry]:
    root = TS_ROOTS[sdk]
    raw = ts_exports(root / "index.ts", set())
    # Explicit and wildcard routes can reach the same symbol. The public name
    # has one slot at the package entry point.
    by_name: dict[str, Entry] = {}
    for entry in raw:
        by_name.setdefault(entry.name, entry)
    return list(by_name.values())


try:
    from dev.sdk.manifest_rules import classify
except ModuleNotFoundError:
    from manifest_rules import classify


def markdown_cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


def build() -> str:
    inventories = {
        "Swift": swift_inventory(),
        "Kotlin": kotlin_inventory(),
        "Node": ts_inventory("Node"),
        "Browser": ts_inventory("Browser"),
    }
    lines = [
        "# SDK API manifest", "",
        "This manifest classifies the public exports of the four current SDKs before they move to the Rust facade. "
        "The [design Ref](https://plan.ref.tools/eG4NJ6emCjsHcWH0) is the authority. "
        "Its Section 11.4 tables are applied by SDK and sub-table. "
        "The implementation plan's Decisions adopt design Section 20 items 1, 2, 3, 4, and 7 and use `end()` for async shutdown. "
        "Final names use stock generator spelling: `ID` suffixes, `unsafe` camel case, string IDs, and one `Timestamp` value with `.ns` and `.date`.", "",
        "`generated` means the facade generator emits the API. `static runtime` means hand-written host code ships with generated output. "
        "`platform helper` means native OS code stays in the SDK. `alias` means a deprecated name kept for one major release (11.5), from a Rename row or an explicit alias row. "
        "`approved removal` means the current export leaves the API. A dash in Final name marks a removal.", "",
        "Symbol grammar: a type or constant is `Name`; a member is `Owner.member`; a free function is `func name`. "
        "A free property is `var name`, `val name`, or `let name`. "
        "Nested owners use dots, such as `Client.Companion.create`. A computed member is `Owner[Symbol.asyncIterator]`. "
        "A named constructor parameter in a public signature uses `Owner.parameter` and Kind `constructor parameter`. "
        "A method may show a call shape in either name column, such as `Client.Companion.register(codec)`, `Group.state().name`, `Client.inboxID(for:)`, or `Conversation.lastActivityAtNs(contentTypes?)`; `Client.inboxID` without parentheses is the field. "
        "An enum value under a record field uses `Record.field.value`, such as `ListMessagesOptions.sortBy.sentAt`. "
        "Kotlin `Client.Companion.register(codec:)` is today's global codec method and is removed; final `Client.register()` registers an identity. "
        "A group row starts `pattern:` and shows a source glob or regular expression plus its declaration count. "
        "The xmtpv3.swift family patterns run in table order after individually listed public-signature `Ffi*` roots are excluded; "
        "each declaration matches the first family only. The final `^.+$` family closes that partition. "
        "A source path and line in Notes distinguish overloads. Kind names the source declaration. "
        "The helper counts source-declared Swift public/open and SPI items, Kotlin public declarations and constructor properties, "
        "and TypeScript package exports plus exported class and object-type members. Compiler-synthesized members are outside this source inventory. "
        "The counts are declaration counts, not table-row counts. Run `python3 dev/sdk/inventory.py --self-test` and `--check` to verify them. "
        "If Section 11.4, another design section, or a plan decision does not cover a symbol, the SDK row gives a proposed status and Open items lists it.", "",
    ]
    counts = {sdk: sum(e.count for e in entries) for sdk, entries in inventories.items()}
    lines += ["| SDK | Public declarations |", "| --- | ---: |"]
    lines += [f"| {sdk} | {count} |" for sdk, count in counts.items()]
    lines += [""]
    open_items: list[str] = []
    for sdk, entries in inventories.items():
        lines += [f"## {sdk}", "", "| Current export | Kind | Final name | Status | Design ref | Notes |", "| --- | --- | --- | --- | --- | --- |"]
        seen: set[str] = set()
        for entry in sorted(entries, key=lambda e: (e.source, e.line)):
            if entry.key in seen:
                raise ValueError(f"duplicate inventory key: {entry.key}")
            seen.add(entry.key)
            result = classify(entry)
            status, final, source_ref, note, is_open = result.status, result.final, result.ref, result.note, result.open
            if status not in {"generated", "static runtime", "platform helper", "alias", "approved removal"}:
                raise ValueError(status)
            current = f"`{entry.display_name}`"
            if entry.count > 1:
                current += f" ({entry.count} declarations)"
            location = entry.source if not entry.line else f"{entry.source}:{entry.line}"
            details = f"{note} Source: `{location}`." if note else f"Source: `{location}`."
            if entry.display_name.startswith("func ") and final != "—" and not final.startswith("func ") and "." not in final:
                final = f"func {final}"
            lines.append("| " + " | ".join(markdown_cell(v) for v in (current, entry.kind, f"`{final}`" if final != "—" else final, status, source_ref, details)) + " |")
            if is_open:
                open_items.append(f"- {sdk} `{entry.display_name}` (`{location}`): proposed **{status}**. {note}")
        lines.append("")
    lines += ["## Open items", ""]
    if open_items:
        lines += [f"{len(open_items)} exports need a design decision. Their proposed status appears in the SDK table.", ""]
        lines += open_items
    else:
        lines.append("None in the source inventory above.")
    lines.append("")
    return "\n".join(lines)


def self_test() -> None:
    swift = swift_scan("@_spi(Unstable) public struct UnstableGroup {\n public func enableProposals() {}\n}\n"
                       "public class Group {\n @_spi(Unstable) public var unstable: UnstableGroup { fatalError() }\n}\n", "fixture.swift")
    assert {entry.name for entry in swift} >= {"UnstableGroup", "UnstableGroup.enableProposals", "Group.unstable"}
    kotlin = kotlin_scan("class DecodedMessage private constructor(\n val id: String\n) {\n fun content() {}\n}\n"
                         "class DecodedMessageV2 private constructor(\n val contentTypeId: String\n) {\n fun refresh() {}\n}\n"
                         "class MessageReader internal constructor(\n val cursor: String\n) {\n fun next() {}\n}\n"
                         "class NotificationError internal constructor(\n val code: String\n) {\n fun description() {}\n}\n"
                         "enum class Kind {\n FIRST,\n SECOND;\n fun value() = when (this) {\n FIRST -> 1\n SECOND -> 2\n }\n}\n"
                         "fun IdentityKind.toFfiPublicIdentifierKind() = 1\n", "fixture.kt")
    names = {entry.name for entry in kotlin}
    assert {"DecodedMessage", "DecodedMessage.id", "DecodedMessage.content", "DecodedMessageV2", "DecodedMessageV2.contentTypeId",
            "MessageReader", "MessageReader.next", "NotificationError", "NotificationError.code", "Kind.FIRST", "Kind.SECOND",
            "IdentityKind.toFfiPublicIdentifierKind"} <= names, names
    assert "Kind.when" not in names, names
    with tempfile.TemporaryDirectory(dir=ROOT) as directory:
        path = Path(directory) / "MessageStream.ts"
        path.write_text("export class MessageStream {\n [Symbol.asyncIterator]() { return this; }\n}\n")
        members = ts_class_members(path, "MessageStream", "Node")
        assert "MessageStream[Symbol.asyncIterator]" in {entry.name for entry in members}
        path.write_text("export type Options = {\n retry?: number;\n nested?: {\n enabled: boolean;\n };\n};\n"
                        "export type Callback = () => void;\nexport class Noise {\n run() {}\n}\n")
        names = {entry.name for entry in ts_object_members(path, "Options", "Node")}
        assert names == {"Options.retry", "Options.nested", "Options.nested.enabled"}, names
        assert not ts_object_members(path, "Callback", "Node")
    inventories = {"Swift": swift_inventory(), "Kotlin": kotlin_inventory(),
                   "Node": ts_inventory("Node"), "Browser": ts_inventory("Browser")}
    expected = {
        "Swift": {
            "Client.create": ("static runtime", "Client.create"),
            "Client.inboxStatesForInboxIds": ("alias", "Client.inboxStates"),
            "Client.keyPackageStatusesForInstallationIds": ("alias", "Client.keyPackageStatuses"),
            "Client.getNewestMessageMetadata": ("alias", "Client.newestMessageMetadata"),
            "Client.verifySignature": ("alias", "Client.verifySignedWithInstallationKey"),
            "Client.getOrCreateInboxId": ("alias", "Client.inboxID(for:)"),
            "Client.libXMTPVersion": ("alias", "Client.libxmtpVersion"),
            "Client.createArchive": ("generated", "Client.archives.exportToFile"),
            "ClientOptions.Api": ("alias", "BackendOptions"),
            "ClientOptions.waitForRegistrationVisible": ("approved removal", "—"),
            "Conversations.newConversationWithIdentity": ("generated", "Conversations.createDm"),
            "Conversations.newGroupCustomPermissionsWithIdentities": ("generated", "Conversations.createGroupWithIdentities"),
            "Group.updateImageUrlPermission": ("generated", "Group.updatePermission"),
            "Group.leaveGroup": ("alias", "Group.requestRemoval"),
            "Group.clearDisappearingMessageSettings": ("generated", "Group.updateDisappearingSettings"),
            "Group.processMessage": ("generated", "Group.processStreamedMessage"),
            "Group.unstable": ("approved removal", "—"),
            "MessageReader.messages": ("static runtime", "MessageReader.stream()"),
            "DecodedMessageV2.contentTypeId": ("static runtime", "Message.contentType"),
            "DecodedMessageV2": ("approved removal", "—"),
            "DecodedMessage": ("alias", "Message"),
            "DecodedMessage.body": ("static runtime", "Message.content"),
            "GroupSyncSummary.numEligible": ("generated", "GroupSyncSummary.eligible"),
            "GroupMembershipState.allowed": ("generated", "GroupMembershipState.allowed"),
            "MessageDeliveryStatus.failed": ("generated", "DeliveryStatus.failed"),
            "MlsExtensionType": ("generated", "MlsExtensionType"),
            "InstallationCapabilities": ("generated", "InstallationCapabilities"),
            "PermissionLevel.Admin": ("generated", "Member.permissionLevel.admin"),
            "MultiRemoteAttachmentError": ("approved removal", "—"),
            "ConsentRecord.entryType": ("generated", "ConsentRecord.entity.kind"),
            "ArchiveOptions.toFfi": ("approved removal", "—"),
            "ConversationDebugInfo.init": ("approved removal", "—"),
            "SignatureRequest.init": ("approved removal", "—"),
            "SignatureRequest.ffiSignatureRequest": ("approved removal", "—"),
            "SignatureRequest.addScwSignature": ("generated", "SignatureRequest.addSignature"),
            "SignatureRequest.addEcdsaSignature": ("generated", "SignatureRequest.addSignature"),
            "FfiXmtpClient.waitForRegistrationVisible": ("generated", "Client.waitForRegistrationVisible"),
        },
        "Kotlin": {
            "Client.Companion.build": ("static runtime", "Client.build"),
            "DecodedMessageV2.contentTypeId": ("static runtime", "Message.contentType"),
            "DecodedMessageV2": ("approved removal", "—"),
            "DecodedMessage": ("alias", "Message"),
            "DecodedMessage.MessageDeliveryStatus.FAILED": ("generated", "DeliveryStatus.failed"),
            "DecodedMessage.MessageDeliveryStatus.ALL": ("approved removal", "—"),
            "DecodedMessage.SortBy.SENT_TIME": ("generated", "ListMessagesOptions.sortBy.sentAt"),
            "Client.Companion.register": ("approved removal", "—"),
            "ClientOptions.waitForRegistrationVisible": ("approved removal", "—"),
            "GroupSyncSummary.numSynced": ("generated", "GroupSyncSummary.synced"),
            "GroupMembershipState.PENDING_REMOVE": ("generated", "GroupMembershipState.pendingRemove"),
            "PermissionLevel.SUPER_ADMIN": ("generated", "Member.permissionLevel.superAdmin"),
            "Throwable.streamFailureDetails": ("approved removal", "—"),
            "PrivatePreferences": ("alias", "Preferences"),
            "PrivatePreferences.client": ("approved removal", "—"),
            "MessageReader.next": ("generated", "MessageReader.next"),
            "Group.updateNamePermission": ("generated", "Group.updatePermission"),
            "Group.addMembersByIdentity": ("generated", "Group.addMembersByIdentity"),
            "IdentityKind.toFfiPublicIdentifierKind": ("approved removal", "—"),
            "EncodedContent.compress": ("approved removal", "—"),
            "ClientOptions.appContext": ("platform helper", "StorageOptions(context)"),
            "PrivatePreferences.syncConsent": ("approved removal", "—"),
            "ArchiveElement.Companion.fromFfi": ("approved removal", "—"),
            "SignatureRequest.ffiSignatureRequest": ("approved removal", "—"),
            "SignatureRequest.addScwSignature": ("generated", "SignatureRequest.addSignature"),
        },
        "Node": {
            "Client.unsafe_createInboxSignatureRequest": ("alias", "Client.unsafeCreateInboxSignatureRequest"),
            "Conversations.fetchDmByIdentifier": ("alias", "Conversations.getDmByIdentity"),
            "DecodedMessage.numReplies": ("static runtime", "Message.replyCount"),
            "Conversation._client": ("approved removal", "—"),
            "Identifier": ("alias", "PublicIdentity"),
            "IdentifierKind": ("approved removal", "—"),
            "SendOpts": ("generated", "SendOptions"),
            "Client.createArchive": ("generated", "Client.archives.exportToFile"),
            "MessageReaderSource.catchUpChanged": ("generated", "MessageReader.catchUpChanged"),
            "MessageReaderSource.conversationType": ("approved removal", "—"),
            "ResolveValue.value": ("static runtime", "ResolveValue.value"),
            "MessageAcknowledgement.reject": ("static runtime", "MessageAcknowledgement.reject"),
            "StreamFailureCause.code": ("generated", "StreamFailureCause.code"),
            "UnfinishedStreamTopic.processed": ("generated", "UnfinishedStreamTopic.processed"),
            "MessageStream[Symbol.asyncIterator]": ("static runtime", "MessageStream[Symbol.asyncIterator]"),
            "OtherOptions.waitForRegistrationVisible": ("approved removal", "—"),
            "Preferences.fetchInboxStates": ("alias", "Client.inboxStates"),
            "StorageOptions.dbEncryptionKey": ("generated", "StorageOptions.encryptionKey"),
            "StreamOptions.retryAttempts": ("approved removal", "—"),
        },
        "Browser": {
            "Client.unsafe_createInboxSignatureText": ("generated", "Client.unsafeCreateInboxSignatureRequest"),
            "Client.unsafe_applySignatureRequest": ("generated", "Client.unsafeApplySignatureRequest(request)"),
            "Conversation.metadata": ("alias", "Conversation.metadata"),
            "DecodedMessage.numReplies": ("static runtime", "Message.replyCount"),
            "MessageStream[Symbol.asyncIterator]": ("static runtime", "MessageStream[Symbol.asyncIterator]"),
            "Identifier": ("alias", "PublicIdentity"),
            "IdentifierKind": ("approved removal", "—"),
            "encryptAttachment": ("alias", "func encryptBytes"),
            "decryptAttachment": ("alias", "func decryptBytes"),
            "Opfs.poolCapacity": ("generated", "StorageAdmin.capacity"),
            "Opfs.listFiles": ("generated", "StorageAdmin.listFiles"),
            "Opfs.fileCount": ("approved removal", "—"),
            "Client.libxmtpVersion": ("generated", "Client.libxmtpVersion"),
            "MessageReaderSource.catchUpChanged": ("generated", "MessageReader.catchUpChanged"),
            "OtherOptions.waitForRegistrationVisible": ("approved removal", "—"),
            "StorageOptions.dbEncryptionKey": ("approved removal", "—"),
            "StreamOptions.retryAttempts": ("approved removal", "—"),
        },
    }
    for sdk, cases in expected.items():
        by_name = {entry.name: entry for entry in inventories[sdk]}
        for current, want in cases.items():
            assert current in by_name, (sdk, current)
            result = classify(by_name[current])
            assert (result.status, result.final) == want, (sdk, current, result, want)
    expected_open = {
        "Swift": {"Client.inMemoryDbPath", "Client.setLibXMTPNativeLogLevel", "Group.addMembersByIdentity", "Conversation.clientInboxId", "FfiXmtpClient.waitForRegistrationVisible"},
        "Kotlin": {"Group.addMembersByIdentity", "ContentTypeIdBuilder", "func encodedContentFromFfi", "func validateInboxId", "ByteArray.toHex", "String.hexToByteArray"},
        "Node": {"OtherOptions.stdoutLoggingLevel"},
        "Browser": {"metadataFieldName"},
    }
    for sdk, names in expected_open.items():
        by_name = {entry.name: entry for entry in inventories[sdk]}
        assert all(name in by_name and classify(by_name[name]).open for name in names), (sdk, names)
    generated_source = SWIFT / "Libxmtp/xmtpv3.swift"
    actual_generated = len(swift_scan(generated_source.read_text(), rel(generated_source)))
    covered_generated = sum(entry.count for entry in inventories["Swift"] if entry.source == rel(generated_source))
    assert actual_generated == covered_generated, (actual_generated, covered_generated)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("inventory fixtures pass")
        return
    output = build()
    if args.write:
        OUT.write_text(output)
    elif args.check:
        if OUT.read_text() != output:
            raise SystemExit("manifest differs from source inventory")
    else:
        print(output)


if __name__ == "__main__":
    main()
