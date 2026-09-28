#!/usr/bin/env python3
"""Compare the public TypeScript declarations of the Node and browser SDKs.

tsc emits declarations for the Node entrypoint and for both browser
entrypoints: the worker bridge and the main-thread pure module. The check
follows every export, value and type-only, to its declaration and compares
the declaration text. Enum members, literal unions, nested fields, optional
markers, and `| undefined` are all in that text. The SDK-037 list and the
exceptions below are the only permitted differences (plan P62).
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
import re
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[3]
GENERATED = ROOT / "target/sdk-generated"
TSC = ROOT / "node_modules/.bin/tsc"

NODE = "typescript-napi"
BROWSER = ("typescript-wasm", "typescript-pure")

# SDK-037 removes these exports from the browser target.
SDK_037_NODE_ONLY = {
    "LogProcessType": "persistent log writer",
    "LogRotation": "persistent log writer",
    "enterDebugWriter": "persistent log writer",
    "exitDebugWriter": "persistent log writer",
    "NotificationChannel": "notifications",
    "NotificationChannel_Tags": "notifications",
    "NotificationConfig": "notifications",
    "NotificationFailure": "notifications",
    "NotificationState": "notifications",
    "NotificationState_Tags": "notifications",
    "decryptFile": "file encryption",
    "encryptFile": "file encryption",
}
# SDK-037 removes these members from the browser target. Each object type is
# listed with its interface and its generated class.
SDK_037_NODE_ONLY_MEMBERS = {
    "ArchivesLike": {"exportToFile", "importFromFile", "metadataFromFile"},
    "Archives": {"exportToFile", "importFromFile", "metadataFromFile"},
    "ClientLike": {"disableNotifications", "enableNotifications", "notificationState"},
    "StorageLike": {"delete_", "reconnect"},
    "Storage": {"delete_", "reconnect"},
    "StorageOptions": {"encryptionKey"},
}
# SDK-037 adds `Storage.admin()` to the browser target. It is not generated
# yet, so the list is empty; add the members when it is.
SDK_037_BROWSER_ONLY_MEMBERS: dict[str, set[str]] = {}

# Exports that are not app API, each with the reason it differs.
INTERNAL_BROWSER_ONLY = {
    # The worker runtime reads it after a failed create. Native builds have
    # no storage lock.
    "storeLeftOpen": "worker runtime only",
    # The main-thread pure module loads its own WASM file.
    "initPureWasm": "loads the main-thread pure module",
}
# Exports that exist on both targets but take a platform-specific form. `None`
# skips the whole declaration; a set skips those members on both targets.
PLATFORM_SPECIFIC: dict[str, set[str] | None] = {
    # Node loads the native library synchronously; the browser needs the
    # URL of its WASM file.
    "uniffiInitAsync": None,
    # The browser host message needs the main-thread session to reach its
    # client. Apps get messages from the SDK and do not construct them.
    "Message": {"constructor"},
}
# Differences that SDK-037 does not list. P62 does not permit them; each one
# needs an owner decision, so the check names it here instead of hiding it.
UNLISTED_DIFFERENCES: dict[str, set[str] | None] = {
    # Node's Client is the runtime wrapper over ClientLike, with static
    # helpers, codecs, and callback listeners. The browser's Client is the
    # generated worker proxy: it implements ClientLike, `create` and `build`
    # take a MainSession, and every async method takes AbortSignal options.
    # The parity type test compares ClientLike instead.
    "Client": None,
    # The browser lifts custom reply bodies inside `content`; Node keeps the
    # reply body as stored and lifts it only in `replyContent`.
    "Message": {"content"},
    # Browser log sink calls cross to the worker, so they are async and take
    # AbortSignal options; Node's are synchronous.
    "setLogSink": None,
    "clearLogSink": None,
}


@dataclass(frozen=True)
class Declaration:
    kind: str
    head: str
    members: tuple[str, ...]


def emit(flavor: str, out: Path) -> Path:
    source = GENERATED / flavor
    subprocess.run(
        [
            str(TSC),
            "--declaration",
            "--emitDeclarationOnly",
            "--noCheck",
            "--skipLibCheck",
            "--target",
            "es2022",
            "--module",
            "preserve",
            "--moduleResolution",
            "bundler",
            "--allowImportingTsExtensions",
            "--rootDir",
            str(source),
            "--outDir",
            str(out / flavor),
            str(source / "index.ts"),
        ],
        check=True,
    )
    return out / flavor


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"^\s*//.*$", "", text, flags=re.M)


def statements(text: str) -> list[list[str]]:
    result: list[list[str]] = []
    for line in strip_comments(text).splitlines():
        if not line.strip():
            continue
        if line[0] not in " \t}])" or not result:
            result.append([line])
        else:
            result[-1].append(line)
    return result


NAMED = re.compile(
    r"^(?:export\s+)?(?:declare\s+)?(?:abstract\s+)?"
    r"(class|interface|enum|function|const|let|type|namespace)\s+([A-Za-z_$][\w$]*)"
)
MEMBER_NAME = re.compile(
    r"^(?:(?:static|readonly|private|protected|abstract|get|set|async)\s+)*"
    r"(#?[A-Za-z_$][\w$]*|\[[^\]]+\])"
)


class Module:
    def __init__(self, path: Path):
        self.path = path
        self.declarations: dict[str, list[list[str]]] = defaultdict(list)
        self.reexports: dict[str, tuple[str, str]] = {}
        self.local_exports: dict[str, str] = {}
        self.stars: list[str] = []
        self.renames: dict[str, str] = {}
        self.imports: dict[str, tuple[str, str]] = {}
        self.namespaces: dict[str, str] = {}
        for statement in statements(path.read_text()):
            self.read(statement)

    def read(self, statement: list[str]) -> None:
        first = " ".join(line.strip() for line in statement)
        if first.startswith("import "):
            if match := re.match(
                r"import (?:type )?\* as (\w+) from ['\"](.+)['\"]", first
            ):
                self.namespaces[match.group(1)] = match.group(2)
            elif match := re.match(
                r"import (?:type )?\{(.*)\} from ['\"](.+)['\"]", first
            ):
                for item in match.group(1).split(","):
                    parts = [
                        part.strip() for part in item.replace("type ", "").split(" as ")
                    ]
                    if not parts[0]:
                        continue
                    self.imports[parts[-1]] = (match.group(2), parts[0])
                    if len(parts) == 2:
                        self.renames[parts[1]] = parts[0]
            return
        if match := re.match(
            r"export (?:type )?\{(.*)\}(?: from ['\"](.+)['\"])?;", first
        ):
            for item in match.group(1).split(","):
                item = item.replace("type ", "").strip()
                if not item:
                    continue
                parts = item.split(" as ")
                local, public = parts[0].strip(), parts[-1].strip()
                if match.group(2):
                    self.reexports[public] = (match.group(2), local)
                else:
                    self.local_exports[public] = local
            return
        if match := re.match(r"export \* from ['\"](.+)['\"];", first):
            self.stars.append(match.group(1))
            return
        if match := re.match(r"export default (\w+);", first):
            self.local_exports["default"] = match.group(1)
            return
        if match := NAMED.match(statement[0]):
            self.declarations[match.group(2)].append(statement)
            if statement[0].startswith("export "):
                self.local_exports[match.group(2)] = match.group(2)

    def normalize(self, text: str) -> str:
        text = re.sub(r"import\([\"'][^\"']*[\"']\)\.", "", text)
        for namespace in self.namespaces:
            text = re.sub(rf"\b{namespace}\.", "", text)
        for alias, original in self.renames.items():
            text = re.sub(rf"\b{re.escape(alias)}\b", original, text)
        text = re.sub(r"^(export\s+)?(declare\s+)?", "", text.strip())
        return re.sub(r"\s+", " ", text)


class Surface:
    def __init__(self, root: Path):
        self.root = root
        self.modules: dict[Path, Module] = {}

    def module(self, path: Path) -> Module:
        path = path.resolve()
        if path not in self.modules:
            self.modules[path] = Module(path)
        return self.modules[path]

    def target(self, module: Module, specifier: str) -> Module:
        base = (module.path.parent / specifier).resolve()
        name = re.sub(r"\.(js|ts)$", "", base.name)
        for candidate in (
            base.parent / f"{name}.d.ts",
            base / "index.d.ts",
        ):
            if candidate.exists():
                return self.module(candidate)
        raise FileNotFoundError(f"{module.path}: cannot resolve {specifier}")

    def exports(self, module: Module) -> dict[str, tuple[Module, str]]:
        result: dict[str, tuple[Module, str]] = {}
        for star in module.stars:
            for name, value in self.exports(self.target(module, star)).items():
                if name != "default":
                    result[name] = value
        for public, local in module.local_exports.items():
            result[public] = (module, local)
        for public, (specifier, local) in module.reexports.items():
            result[public] = self.resolve(self.target(module, specifier), local)
        return result

    def resolve(self, module: Module, name: str) -> tuple[Module, str]:
        exported = self.exports(module)
        if name not in exported:
            raise KeyError(f"{module.path}: no export {name}")
        return exported[name]

    def base(self, module: Module, reference: str) -> tuple[Module, str] | None:
        namespace, _, name = reference.rpartition(".")
        try:
            if namespace in module.namespaces:
                return self.resolve(
                    self.target(module, module.namespaces[namespace]), name
                )
            if name in module.imports:
                specifier, original = module.imports[name]
                return self.resolve(self.target(module, specifier), original)
        except FileNotFoundError:
            return None
        if name in module.declarations:
            return module, name
        return None

    def declaration(self, module: Module, name: str) -> list[Declaration]:
        result = []
        for statement in module.declarations.get(name, []):
            match = NAMED.match(statement[0])
            assert match is not None
            kind = match.group(1)
            if kind not in ("class", "interface", "enum") or not statement[
                0
            ].rstrip().endswith("{"):
                result.append(
                    Declaration(kind, module.normalize("\n".join(statement)), ())
                )
                continue
            # Compare members as a set: their order is not API.
            members: list[str] = []
            for line in statement[1:-1]:
                starts = re.match(r"    [^\s})\]|&]", line) is not None
                if starts or not members:
                    members.append(line)
                else:
                    members[-1] += line
            members = [
                module.normalize(member)
                for member in members
                if not re.match(r"\s*(private\s|#private)", member)
            ]
            head = statement[0]
            # A class that extends another declared class shows the members it
            # inherits, so the two targets compare the whole public class.
            extends = (
                re.search(r"\bextends ([\w$.]+)", head) if kind == "class" else None
            )
            base = self.base(module, extends.group(1)) if extends else None
            if extends and base is not None:
                head = head.replace(extends.group(0), "").replace("  ", " ")
                own = {member_name(member) for member in members}
                for inherited in self.declaration(*base):
                    members += [
                        m for m in inherited.members if member_name(m) not in own
                    ]
            head = module.normalize(head) + " ... " + statement[-1].strip()
            result.append(Declaration(kind, head, tuple(sorted(members))))
        if not result:
            raise KeyError(f"{module.path}: no declaration for {name}")
        return result


def member_name(member: str) -> str:
    match = MEMBER_NAME.match(member)
    return match.group(1) if match else member


def without(declarations: list[Declaration], names: set[str]) -> list[Declaration]:
    result = []
    for item in declarations:
        members = tuple(m for m in item.members if member_name(m) not in names)
        head = item.head
        if not item.members and item.kind in ("type", "const"):
            for name in names:
                # The removed field, and its key in the record factory types.
                head = re.sub(rf"\s{re.escape(name)}\??: [^;]*;", "", head)
                head = re.sub(
                    rf'"{re.escape(name)}" \| |\s\| "{re.escape(name)}"', "", head
                )
        result.append(Declaration(item.kind, head, members))
    return sorted(result, key=lambda item: (item.kind, item.head))


def describe(declarations: list[Declaration]) -> str:
    lines = []
    for item in declarations:
        lines.append(f"  {item.head}")
        lines.extend(f"    {member}" for member in item.members)
    return "\n".join(lines)


def compare(out: Path) -> list[str]:
    node_root = emit(NODE, out)
    node = Surface(node_root)
    node_exports = node.exports(node.module(node_root / "index.d.ts"))
    browser: dict[str, list[tuple[str, Module, str]]] = defaultdict(list)
    surfaces: dict[str, Surface] = {}
    for flavor in BROWSER:
        root = emit(flavor, out)
        surface = surfaces[flavor] = Surface(root)
        for name, (module, local) in surface.exports(
            surface.module(root / "index.d.ts")
        ).items():
            browser[name].append((flavor, module, local))

    errors = []
    for name in sorted(node_exports.keys() - browser.keys()):
        if name not in SDK_037_NODE_ONLY:
            errors.append(f"{name}: Node exports it and the browser does not")
    for name in sorted(browser.keys() - node_exports.keys()):
        if name not in INTERNAL_BROWSER_ONLY:
            errors.append(f"{name}: the browser exports it and Node does not")
    for name in sorted(SDK_037_NODE_ONLY.keys() & browser.keys()):
        errors.append(f"{name}: SDK-037 removes it from the browser")
    for name in sorted(node_exports.keys() & browser.keys()):
        skipped = set()
        for exceptions in (PLATFORM_SPECIFIC, UNLISTED_DIFFERENCES):
            if name in exceptions:
                if exceptions[name] is None:
                    skipped = None
                    break
                skipped |= exceptions[name]
        if skipped is None:
            continue
        module, local = node_exports[name]
        native = without(
            node.declaration(module, local),
            SDK_037_NODE_ONLY_MEMBERS.get(name, set()) | skipped,
        )
        for flavor, web_module, web_local in browser[name]:
            web = without(
                surfaces[flavor].declaration(web_module, web_local),
                SDK_037_BROWSER_ONLY_MEMBERS.get(name, set()) | skipped,
            )
            if native != web:
                errors.append(
                    f"{name}: Node and {flavor} declarations differ\n"
                    f" Node:\n{describe(native)}\n {flavor}:\n{describe(web)}"
                )
    return errors


def main() -> None:
    with tempfile.TemporaryDirectory() as out:
        errors = compare(Path(out))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("Node and browser public declarations match except the listed differences")


if __name__ == "__main__":
    main()
