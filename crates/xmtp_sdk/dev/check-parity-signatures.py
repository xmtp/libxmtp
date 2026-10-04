#!/usr/bin/env python3
"""Compare the public TypeScript declarations of the Node and browser SDKs.

tsc emits declarations for the three package roots: Node, the browser worker
package, and the browser pure module. The check follows every export, value
and type-only, to its declaration and compares the declaration text. Enum
members, literal unions, nested fields, optional markers, and `| undefined`
are all in that text. The SDK-037 list, the pure-module split, and the lists
below are the only permitted differences (plan P62).

The two browser roots split the surface: the worker package exports every
Node export except SDK-037 and the pure-module names, and the pure module
exports the pure-module names with the Node declarations.
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
WORKER = "typescript-wasm"
PURE = "typescript-pure"

# SDK-037 removes these exports from the browser target.
SDK_037_NODE_ONLY = {
    "LogProcessType": "persistent log writer",
    "LogRotation": "persistent log writer",
    "enterDebugWriter": "persistent log writer",
    "exitDebugWriter": "persistent log writer",
    "NotificationChannel": "notifications",
    "NotificationConfig": "notifications",
    "NotificationFailure": "notifications",
    "NotificationState": "notifications",
    "decryptFile": "file encryption",
    "encryptFile": "file encryption",
}
# The main-thread pure module owns the codecs and the pure helpers. The worker
# package does not export them.
PURE_ONLY = {
    "ActionsCodec",
    "AttachmentCodec",
    "catalogueContentTypeShouldPush",
    "DeleteMessageCodec",
    "GroupUpdatedCodec",
    "IntentCodec",
    "LeaveRequestCodec",
    "MarkdownCodec",
    "MultiRemoteAttachmentCodec",
    "ReactionV2Codec",
    "ReadReceiptCodec",
    "RemoteAttachmentCodec",
    "ReplyCodec",
    "TextCodec",
    "TransactionReferenceCodec",
    "WalletSendCallsCodec",
    "decodeEncodedContent",
    "decodeStandard",
    "encodeEncodedContent",
    "encodeStandard",
    "encodeText",
    "isCatalogueContentType",
    "metadataFieldRef",
    "remoteAttachmentFromEncrypted",
    "sdkVersion",
    "standardContentType",
}
# The package roots (compare_public). SDK-037 and the pure module
# split these names from the browser worker entry, each with its reason.
PUBLIC_NODE_ONLY = {
    **SDK_037_NODE_ONLY,
    **{name: "pure module" for name in PURE_ONLY},
    "resumeStreams": "native stream lifecycle",
    "suspendStreams": "native stream lifecycle",
}
# Members whose form depends on the target, skipped on both sides.
PUBLIC_PLATFORM_SPECIFIC: dict[str, str] = {}
PUBLIC_BROWSER_ONLY = {"StorageAdmin": "SDK-037 browser storage admin"}
PUBLIC_NODE_ONLY_MEMBERS = {
    "Archives": {"exportToFile", "importFromFile", "metadataFromFile"},
    "Client": {"disableNotifications", "enableNotifications", "notificationState"},
    "Storage": {"delete_", "reconnect"},
    "StorageOptions": {"encryptionKey"},
}
PUBLIC_BROWSER_ONLY_MEMBERS = {"Storage": {"admin"}}


@dataclass(frozen=True)
class Declaration:
    kind: str
    head: str
    members: tuple[str, ...]


def emit(flavor: str, out: Path, entry: str = "index.ts") -> Path:
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
            # One root for every flavor: a flavor imports files from another
            # (WASM from pure), and tsc writes a file outside the root next to
            # its source, inside the generated tree.
            "--rootDir",
            str(GENERATED),
            "--outDir",
            str(out),
            str(source / entry),
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
            # `import { X } from "..."; export { X };` exports the imported
            # declaration.
            if local in module.imports and local not in module.declarations:
                specifier, original = module.imports[local]
                result[public] = self.resolve(self.target(module, specifier), original)
            else:
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
                head = re.sub(
                    rf"\s(?:readonly )?{re.escape(name)}\??: [^;]*;", "", head
                )
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


def compare_public(out: Path) -> list[str]:
    """Compare the package roots (`index.ts`) of Node and the browser worker
    bridge. The browser entry omits the SDK-037 and pure-module
    exports and adds the browser storage admin; every shared declaration must
    match, member for member."""
    node_root = emit(NODE, out, "index.ts")
    node = Surface(node_root)
    node_exports = node.exports(node.module(node_root / "index.d.ts"))
    web_root = emit(WORKER, out, "index.ts")
    web = Surface(web_root)
    web_exports = web.exports(web.module(web_root / "index.d.ts"))
    errors = []
    expected = (
        node_exports.keys() - PUBLIC_NODE_ONLY.keys()
    ) | PUBLIC_BROWSER_ONLY.keys()
    for name in sorted(PUBLIC_NODE_ONLY.keys() - node_exports.keys()):
        errors.append(
            f"{name}: public Node-only list names it and Node does not export it"
        )
    for name in sorted(expected - web_exports.keys()):
        errors.append(
            f"{name}: the Node public entry exports it and the browser does not"
        )
    for name in sorted(web_exports.keys() - expected):
        errors.append(f"{name}: the browser public entry exports it outside its list")
    for name in sorted(expected & web_exports.keys() & node_exports.keys()):
        if name in PUBLIC_PLATFORM_SPECIFIC:
            continue
        skipped = PUBLIC_NODE_ONLY_MEMBERS.get(name, set())
        native = without(node.declaration(*node_exports[name]), skipped)
        browser = without(
            web.declaration(*web_exports[name]),
            PUBLIC_BROWSER_ONLY_MEMBERS.get(name, set()),
        )
        if native != browser:
            errors.append(
                f"{name}: Node and browser public declarations differ\n"
                f" Node:\n{describe(native)}\n browser:\n{describe(browser)}"
            )
    return errors


# The pure module's own export: it loads the main-thread pure WASM file.
PURE_PUBLIC_ONLY = {"initPureWasm": "loads the main-thread pure module"}


def compare_pure(out: Path) -> list[str]:
    """Compare the pure module's root with the Node root.
    Every pure export except its WASM loader is a Node public name, and each
    one must have the Node declaration. Every Node public name that the pure
    binding defines, and every pure-module name, must be exported."""
    node_root = emit(NODE, out, "index.ts")
    node = Surface(node_root)
    node_exports = node.exports(node.module(node_root / "index.d.ts"))
    pure_root = emit(PURE, out, "index.ts")
    pure = Surface(pure_root)
    pure_exports = pure.exports(pure.module(pure_root / "index.d.ts"))
    # The pure binding decides which Node public names belong in pure. Each
    # one must reach the pure root, so a name the generator drops fails here.
    binding_root = emit(PURE, out, "xmtp_sdk.ts")
    binding = Surface(binding_root)
    binding_exports = binding.exports(binding.module(binding_root / "xmtp_sdk.d.ts"))
    expected = (
        (binding_exports.keys() & node_exports.keys())
        | PURE_ONLY
        | PURE_PUBLIC_ONLY.keys()
    )
    errors = []
    for name in sorted(expected - pure_exports.keys()):
        errors.append(
            f"{name}: the pure binding defines this Node public name and the "
            "pure public entry does not export it"
        )
    shared = pure_exports.keys() - PURE_PUBLIC_ONLY.keys()
    for name in sorted(shared - node_exports.keys()):
        errors.append(f"{name}: the pure public entry exports it and Node does not")
    for name in sorted(shared & node_exports.keys()):
        native = node.declaration(*node_exports[name])
        browser = pure.declaration(*pure_exports[name])
        if native != browser:
            errors.append(
                f"{name}: Node and pure public declarations differ\n"
                f" Node:\n{describe(native)}\n pure:\n{describe(browser)}"
            )
    return errors


def main() -> None:
    with tempfile.TemporaryDirectory() as out:
        errors = compare_public(Path(out))
    with tempfile.TemporaryDirectory() as out:
        errors += compare_pure(Path(out))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("Node and browser public declarations match except the listed differences")


if __name__ == "__main__":
    main()
