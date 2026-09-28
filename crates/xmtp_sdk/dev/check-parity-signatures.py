#!/usr/bin/env python3
"""Compare the public TypeScript declarations of the Node and browser SDKs.

tsc emits declarations for the Node entrypoint and for both browser
entrypoints: the worker bridge and the main-thread pure module. The check
follows every export, value and type-only, to its declaration and compares
the declaration text. Enum members, literal unions, nested fields, optional
markers, and `| undefined` are all in that text. The SDK-037 list and the
exceptions below are the only permitted differences (plan P62).

The two browser entrypoints split the surface. Each one is checked on its
own: the worker bridge exports every Node export except SDK-037 and the
pure-only list, and the pure module exports exactly the pure lists.
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
BROWSER = (WORKER, PURE)

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
# SDK-037 adds the browser-only admin handle. Its public factory is added
# by the browser adapter. The stock interface alias follows the generated type.
SDK_037_BROWSER_ONLY = {"StorageAdmin", "StorageAdminLike", "StorageAdminInterface"}

# SDK-037 adds `Storage.admin()` to the browser target. It is not generated
# yet, so the list is empty; add the members when it is.
SDK_037_BROWSER_ONLY_MEMBERS: dict[str, set[str]] = {}

# Exports that are not app API, each with the one browser entrypoint that
# exports it and the reason it differs.
INTERNAL_BROWSER_ONLY = {
    # The worker runtime reads it after a failed storage call. Native builds have
    # no storage lock.
    "storageRequiresWorkerRestart": (WORKER, "worker runtime only"),
    "prepareStorageForShutdown": (WORKER, "final worker cleanup only"),
    # The main-thread pure module loads its own WASM file.
    "initPureWasm": (PURE, "loads the main-thread pure module"),
}
# The main-thread pure module owns the codecs and the pure helpers. The worker
# bridge does not export them.
PURE_ONLY = {
    "ActionsCodec",
    "AttachmentCodec",
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
    "decodeStandard",
    "encodeStandard",
    "encodeText",
    "sdkVersion",
    "standardContentType",
}
# The records, enums, and errors that the pure module shares with the worker
# bridge. With PURE_ONLY and its internal exports, this is the whole pure
# module surface.
PURE_SHARED = {
    "Action",
    "ActionStyle",
    "Actions",
    "Attachment",
    "Compression",
    "ContentTypeId",
    "ConversationId",
    "DeletedBy",
    "DeletedBy_Tags",
    "DeletedMessage",
    "EncodedContent",
    "ErrorCategory",
    "ErrorDetails",
    "GroupUpdated",
    "InboxId",
    "InstallationId",
    "Intent",
    "LeaveRequest",
    "MessageId",
    "MetadataFieldChange",
    "MultiRemoteAttachment",
    "Reaction",
    "ReactionAction",
    "ReactionSchema",
    "RemoteAttachment",
    "SendOptions",
    "StandardContent",
    "StandardContentKind",
    "StandardContent_Tags",
    "Timestamp",
    "TransactionMetadata",
    "TransactionReference",
    "WalletCall",
    "WalletCallMetadata",
    "WalletSendCalls",
    "XmtpError",
    "XmtpError_Tags",
    "default",
    "uniffiInitAsync",
}
# Members that exist on both targets but take a platform-specific form. The
# check skips these members on both targets.
PLATFORM_SPECIFIC: dict[str, set[str]] = {
    # The browser host message needs the main-thread session to reach its
    # client. Apps get messages from the SDK and do not construct them.
    "Message": {"constructor"},
}
# Differences that the owner permitted on 2026-09-28 (plan decision O13), in
# addition to the SDK-037 list, until the browser surface is aligned with
# Node. Remove an entry when its difference is fixed.
OWNER_PERMITTED: dict[str, set[str]] = {
    # The browser lifts custom reply bodies inside `content`; Node keeps the
    # reply body as stored and lifts it only in `replyContent`.
    "Message": {"content"},
}
# The pinned Client declarations, as `describe` prints them without indent.
CLIENT_NODE = """\
class Client { ... }
conversations(): ConversationsLike;
decodeCustom(encoded: EncodedContent): { value?: unknown; error?: string; } | undefined;
end(): Promise<void>;
events(filter: EventFilter): Promise<EventStream>;
inboxId(): InboxId;
installationId(): InstallationId;
readonly raw: ClientLike;
startListener(filter: EventFilter, callback: (event: ClientEvent) => void | Promise<void>): Promise<bigint>;
static build(identity: PublicIdentity, options: SDKClientOptions, inboxId?: InboxId): Promise<Client>;
static canMessage(identities: PublicIdentity[], backend: BackendSource): Promise<Map<string, boolean>>;
static create(signer: Signer, options: SDKClientOptions): Promise<Client>;
static fetchServerConfiguration(backend: BackendSource): Promise<ServerConfiguration>;
static inboxIdFor(identity: PublicIdentity, backend: BackendSource): Promise<InboxId>;
static inboxStates(ids: InboxId[], backend: BackendSource): Promise<InboxState[]>;
static isAddressAuthorized(inboxId: InboxId, address: string, backend: BackendSource): Promise<boolean>;
static isInstallationAuthorized(inboxId: InboxId, installationId: InstallationId, backend: BackendSource): Promise<boolean>;
static keyPackageStatuses(ids: InstallationId[], backend: BackendSource): Promise<Map<string, KeyPackageStatus>>;
static newestMessageMetadata(ids: ConversationId[], backend: BackendSource): Promise<Map<string, MessageMetadataEntry>>;
static revokeInstallations(signer: Signer, inboxId: InboxId, ids: InstallationId[], backend: BackendSource): Promise<void>;
static verifySignedWithPublicKey(text: string, signature: ArrayBuffer, publicKey: ArrayBuffer): Promise<boolean>;
stopListener(id: bigint): Promise<void>;
storage(): StorageLike;
"""
CLIENT_WORKER = """\
class Client implements ClientLike { ... }
appVersion(): string | undefined;
archives(): ArchivesLike;
canMessage(identities: Array<PublicIdentity>, asyncOpts_?: { signal: AbortSignal; }): Promise<Map<string, boolean>>;
catchUpToLive(timeoutMs: bigint | undefined, asyncOpts_?: { signal: AbortSignal; }): Promise<CatchUpSummary>;
changeRecoveryIdentifier(signer: Signer, identity: PublicIdentity, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
checkLive(name: string, session?: MainSession): void;
clientKey(): bigint;
constructor(session: MainSession, handle: HandleWire);
conversations(): ConversationsLike;
decodeContent(encoded: EncodedContent, asyncOpts_?: { signal: AbortSignal; }): Promise<MessageContent>;
diagnostics(): DiagnosticsLike;
end(asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
events(filter: EventFilter, asyncOpts_?: { signal: AbortSignal; }): Promise<EventReaderLike>;
identity(): PublicIdentity;
inboxId(): string;
inboxIdFor(identity: PublicIdentity, asyncOpts_?: { signal: AbortSignal; }): Promise<string | undefined>;
inboxState(refreshFromNetwork: boolean, asyncOpts_?: { signal: AbortSignal; }): Promise<InboxState>;
inboxStates(ids: Array<string>, refreshFromNetwork: boolean, asyncOpts_?: { signal: AbortSignal; }): Promise<Array<InboxState>>;
installationId(): string;
installationIdBytes(): ArrayBuffer;
isInMemory(): boolean;
isRegistered(asyncOpts_?: { signal: AbortSignal; }): Promise<boolean>;
keyPackageStatuses(ids: Array<string>, asyncOpts_?: { signal: AbortSignal; }): Promise<Map<string, KeyPackageStatus>>;
latestInboxUpdatesCount(ids: Array<string>, refreshFromNetwork: boolean, asyncOpts_?: { signal: AbortSignal; }): Promise<Map<string, bigint>>;
libxmtpVersion(): string;
options(): ClientOptions;
ownInboxUpdatesCount(refreshFromNetwork: boolean, asyncOpts_?: { signal: AbortSignal; }): Promise<bigint>;
preferences(): PreferencesLike;
protected call(key: string, args: unknown[] | (() => unknown[]), signal?: AbortSignal): Promise<unknown>;
protected check(): void;
protected fence(): void;
protected readonly session: MainSession;
protected snapshot(name: string): unknown;
protected unfence(): void;
readonly handle: HandleWire;
refreshServerConfiguration(asyncOpts_?: { signal: AbortSignal; }): Promise<ServerConfiguration>;
register(asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
release(): void;
removeAccount(recoverySigner: Signer, identity: PublicIdentity, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
revokeAllOtherInstallations(signer: Signer, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
revokeInstallations(signer: Signer, ids: Array<string>, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
serverConfiguration(): ServerConfiguration;
setCredential(credential: Credential, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
signWithInstallationKey(text: string, asyncOpts_?: { signal: AbortSignal; }): Promise<ArrayBuffer>;
startListener(filter: EventFilter, listener: EventListener, asyncOpts_?: { signal: AbortSignal; }): Promise<ListenerId>;
static build(session: MainSession, identity: PublicIdentity, options: HostClientOptions, inboxId: string | undefined, asyncOpts_?: { signal: AbortSignal; }): Promise<Client>;
static create(session: MainSession, signer: Signer, options: HostClientOptions, asyncOpts_?: { signal: AbortSignal; }): Promise<Client>;
stopListener(id: ListenerId, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
storage(): StorageLike;
storagePath(): string | undefined;
syncAllDeviceSyncGroups(asyncOpts_?: { signal: AbortSignal; }): Promise<GroupSyncSummary>;
unsafeAddAccount(signer: Signer, allowInboxReassign: boolean, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
unsafeAddAccountSignatureRequest(identity: PublicIdentity, allowInboxReassign: boolean, asyncOpts_?: { signal: AbortSignal; }): Promise<SignatureRequestLike>;
unsafeApplySignatureRequest(request: SignatureRequestLike, asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
unsafeChangeRecoveryIdentifierSignatureRequest(identity: PublicIdentity, asyncOpts_?: { signal: AbortSignal; }): Promise<SignatureRequestLike>;
unsafeCreateInboxSignatureRequest(asyncOpts_?: { signal: AbortSignal; }): Promise<SignatureRequestLike | undefined>;
unsafeRemoveAccountSignatureRequest(identity: PublicIdentity, asyncOpts_?: { signal: AbortSignal; }): Promise<SignatureRequestLike>;
unsafeRevokeAllOtherInstallationsSignatureRequest(asyncOpts_?: { signal: AbortSignal; }): Promise<SignatureRequestLike | undefined>;
unsafeRevokeInstallationsSignatureRequest(ids: Array<string>, asyncOpts_?: { signal: AbortSignal; }): Promise<SignatureRequestLike>;
verifySignedWithInstallationKey(text: string, signature: ArrayBuffer, asyncOpts_?: { signal: AbortSignal; }): Promise<boolean>;
"""
# Declarations that differ as a whole. Each target must match its pinned text,
# as `describe` prints it, so any drift on either target fails. When a pinned
# declaration changes on purpose, copy the new text from the error.
PINNED: dict[str, dict[str, str]] = {
    # Node loads the native library; the browser needs the URL of its WASM
    # file.
    "uniffiInitAsync": {
        NODE: "function uniffiInitAsync(): Promise<void>;",
        WORKER: "function uniffiInitAsync(source: WasmSource): Promise<void>;",
        PURE: "function uniffiInitAsync(source: WasmSource): Promise<void>;",
    },
    # Owner decision O13 (2026-09-28). Browser log sink calls cross to the
    # worker, so they are async and take AbortSignal options; Node's are
    # synchronous.
    "setLogSink": {
        NODE: "function setLogSink(sink?: LogSink): void;",
        WORKER: "function setLogSink(sink: LogSink | undefined, asyncOpts_?: "
        "{ signal: AbortSignal; }): Promise<void>;",
    },
    "clearLogSink": {
        NODE: "function clearLogSink(): void;",
        WORKER: "function clearLogSink(asyncOpts_?: { signal: AbortSignal; }): "
        "Promise<void>;",
    },
    # Owner decision O13 (2026-09-28). Node's Client is the runtime wrapper
    # over ClientLike, with static helpers, codecs, and callback listeners.
    # The browser's Client is the generated worker proxy: it implements
    # ClientLike, `create` and `build` take a MainSession, and every async
    # method takes AbortSignal options. The parity type test compares
    # ClientLike.
    "Client": {
        NODE: CLIENT_NODE,
        WORKER: CLIENT_WORKER,
    },
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
            # One root for every flavor: a flavor imports files from another
            # (WASM from pure), and tsc writes a file outside the root next to
            # its source, inside the generated tree.
            "--rootDir",
            str(GENERATED),
            "--outDir",
            str(out),
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


def pinned_text(text: str) -> str:
    return "\n".join(line.strip() for line in text.strip().splitlines())


def expected_exports(flavor: str, node_exports: set[str]) -> set[str]:
    if flavor == PURE:
        return PURE_ONLY | PURE_SHARED
    return (node_exports - SDK_037_NODE_ONLY.keys() - PURE_ONLY) | SDK_037_BROWSER_ONLY


def compare(out: Path) -> list[str]:
    node_root = emit(NODE, out)
    node = Surface(node_root)
    node_exports = node.exports(node.module(node_root / "index.d.ts"))
    errors = []
    for name in sorted(SDK_037_BROWSER_ONLY & node_exports.keys()):
        errors.append(f"{name}: SDK-037 permits it only in the browser")
    for name in sorted((PURE_ONLY | PURE_SHARED) - node_exports.keys()):
        errors.append(f"{name}: the pure list names it and Node does not export it")
    for flavor in BROWSER:
        root = emit(flavor, out)
        surface = Surface(root)
        exports = surface.exports(surface.module(root / "index.d.ts"))
        internal = {
            name
            for name, (owner, _reason) in INTERNAL_BROWSER_ONLY.items()
            if owner == flavor
        }
        expected = expected_exports(flavor, node_exports.keys())
        for name in sorted(expected - exports.keys()):
            errors.append(f"{name}: Node exports it and {flavor} does not")
        for name in sorted(exports.keys() - expected - internal):
            if name in SDK_037_NODE_ONLY:
                errors.append(f"{name}: SDK-037 removes it from {flavor}")
            elif name in INTERNAL_BROWSER_ONLY or name not in node_exports:
                errors.append(f"{name}: {flavor} exports it and Node does not")
            else:
                errors.append(f"{name}: {flavor} exports it outside its list")
        for name in sorted(internal - exports.keys()):
            errors.append(f"{name}: the internal export is missing from {flavor}")
        for name in sorted(expected & exports.keys()):
            if name in SDK_037_BROWSER_ONLY:
                continue
            web_module, web_local = exports[name]
            if name in PINNED:
                for target, surface_of, module, local in (
                    (NODE, node, *node_exports[name]),
                    (flavor, surface, web_module, web_local),
                ):
                    pin = PINNED[name].get(target)
                    actual = pinned_text(
                        describe(surface_of.declaration(module, local))
                    )
                    if pin is None or pinned_text(pin) != actual:
                        errors.append(
                            f"{name}: the {target} declaration differs from its"
                            f" pin\n{actual}"
                        )
                continue
            skipped = PLATFORM_SPECIFIC.get(name, set()) | OWNER_PERMITTED.get(
                name, set()
            )
            module, local = node_exports[name]
            native = without(
                node.declaration(module, local),
                SDK_037_NODE_ONLY_MEMBERS.get(name, set()) | skipped,
            )
            web = without(
                surface.declaration(web_module, web_local),
                SDK_037_BROWSER_ONLY_MEMBERS.get(name, set()) | skipped,
            )
            if native != web:
                errors.append(
                    f"{name}: Node and {flavor} declarations differ\n"
                    f" Node:\n{describe(native)}\n {flavor}:\n{describe(web)}"
                )
    # Each flavor checks the Node side of a pin again; report it once.
    return list(dict.fromkeys(errors))


def main() -> None:
    with tempfile.TemporaryDirectory() as out:
        errors = compare(Path(out))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("Node and browser public declarations match except the listed differences")


if __name__ == "__main__":
    main()
