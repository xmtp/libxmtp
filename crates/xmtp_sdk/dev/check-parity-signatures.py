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

# SDK-037 adds `Storage.admin()` to the browser target. The whole Storage
# declaration is pinned below, so no member is listed here.
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
# The private public entries (compare_public). SDK-037 and the pure module
# split these names from the browser worker entry, each with its reason.
PUBLIC_NODE_ONLY = {
    **{
        name: reason
        for name, reason in SDK_037_NODE_ONLY.items()
        if not name.endswith("_Tags")
    },
    "setLogSink": "F7 adds the asynchronous browser log sink",
    "LogSink": "F7 adds the asynchronous browser log sink",
    **{name: "pure module" for name in PURE_ONLY},
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

# The pinned Client declarations, as `describe` prints them without indent.
CLIENT_NODE = """\
class Client { ... }
appVersion(...args: Parameters<ClientLike["appVersion"]>): ReturnType<ClientLike["appVersion"]>;
archives(...args: Parameters<ClientLike["archives"]>): ReturnType<ClientLike["archives"]>;
catchUpToLive(...args: Parameters<ClientLike["catchUpToLive"]>): ReturnType<ClientLike["catchUpToLive"]>;
changeRecoveryIdentifier(...args: Parameters<ClientLike["changeRecoveryIdentifier"]>): ReturnType<ClientLike["changeRecoveryIdentifier"]>;
conversations(...args: Parameters<ClientLike["conversations"]>): ReturnType<ClientLike["conversations"]>;
decodeContent(...args: Parameters<ClientLike["decodeContent"]>): ReturnType<ClientLike["decodeContent"]>;
decodeCustom(encoded: EncodedContent): { value?: unknown; error?: string; } | undefined;
diagnostics(...args: Parameters<ClientLike["diagnostics"]>): ReturnType<ClientLike["diagnostics"]>;
disableNotifications(...args: Parameters<ClientLike["disableNotifications"]>): ReturnType<ClientLike["disableNotifications"]>;
enableNotifications(...args: Parameters<ClientLike["enableNotifications"]>): ReturnType<ClientLike["enableNotifications"]>;
end(): Promise<void>;
events(filter: EventFilter): Promise<EventStream>;
identity(...args: Parameters<ClientLike["identity"]>): ReturnType<ClientLike["identity"]>;
inboxId(...args: Parameters<ClientLike["inboxId"]>): ReturnType<ClientLike["inboxId"]>;
inboxState(...args: Parameters<ClientLike["inboxState"]>): ReturnType<ClientLike["inboxState"]>;
installationId(...args: Parameters<ClientLike["installationId"]>): ReturnType<ClientLike["installationId"]>;
installationIdBytes(...args: Parameters<ClientLike["installationIdBytes"]>): ReturnType<ClientLike["installationIdBytes"]>;
isInMemory(...args: Parameters<ClientLike["isInMemory"]>): ReturnType<ClientLike["isInMemory"]>;
isRegistered(...args: Parameters<ClientLike["isRegistered"]>): ReturnType<ClientLike["isRegistered"]>;
latestInboxUpdatesCount(...args: Parameters<ClientLike["latestInboxUpdatesCount"]>): ReturnType<ClientLike["latestInboxUpdatesCount"]>;
libxmtpVersion(...args: Parameters<ClientLike["libxmtpVersion"]>): ReturnType<ClientLike["libxmtpVersion"]>;
notificationState(...args: Parameters<ClientLike["notificationState"]>): ReturnType<ClientLike["notificationState"]>;
options(...args: Parameters<ClientLike["options"]>): ReturnType<ClientLike["options"]>;
ownInboxUpdatesCount(...args: Parameters<ClientLike["ownInboxUpdatesCount"]>): ReturnType<ClientLike["ownInboxUpdatesCount"]>;
preferences(...args: Parameters<ClientLike["preferences"]>): ReturnType<ClientLike["preferences"]>;
protected binding(): ClientLike;
refreshServerConfiguration(...args: Parameters<ClientLike["refreshServerConfiguration"]>): ReturnType<ClientLike["refreshServerConfiguration"]>;
register(...args: Parameters<ClientLike["register"]>): ReturnType<ClientLike["register"]>;
removeAccount(...args: Parameters<ClientLike["removeAccount"]>): ReturnType<ClientLike["removeAccount"]>;
revokeAllOtherInstallations(...args: Parameters<ClientLike["revokeAllOtherInstallations"]>): ReturnType<ClientLike["revokeAllOtherInstallations"]>;
serverConfiguration(...args: Parameters<ClientLike["serverConfiguration"]>): ReturnType<ClientLike["serverConfiguration"]>;
setCredential(...args: Parameters<ClientLike["setCredential"]>): ReturnType<ClientLike["setCredential"]>;
signWithInstallationKey(...args: Parameters<ClientLike["signWithInstallationKey"]>): ReturnType<ClientLike["signWithInstallationKey"]>;
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
storagePath(...args: Parameters<ClientLike["storagePath"]>): ReturnType<ClientLike["storagePath"]>;
syncAllDeviceSyncGroups(...args: Parameters<ClientLike["syncAllDeviceSyncGroups"]>): ReturnType<ClientLike["syncAllDeviceSyncGroups"]>;
unsafeAddAccount(...args: Parameters<ClientLike["unsafeAddAccount"]>): ReturnType<ClientLike["unsafeAddAccount"]>;
unsafeAddAccountSignatureRequest(...args: Parameters<ClientLike["unsafeAddAccountSignatureRequest"]>): ReturnType<ClientLike["unsafeAddAccountSignatureRequest"]>;
unsafeApplySignatureRequest(...args: Parameters<ClientLike["unsafeApplySignatureRequest"]>): ReturnType<ClientLike["unsafeApplySignatureRequest"]>;
unsafeChangeRecoveryIdentifierSignatureRequest(...args: Parameters<ClientLike["unsafeChangeRecoveryIdentifierSignatureRequest"]>): ReturnType<ClientLike["unsafeChangeRecoveryIdentifierSignatureRequest"]>;
unsafeCreateInboxSignatureRequest(...args: Parameters<ClientLike["unsafeCreateInboxSignatureRequest"]>): ReturnType<ClientLike["unsafeCreateInboxSignatureRequest"]>;
unsafeRemoveAccountSignatureRequest(...args: Parameters<ClientLike["unsafeRemoveAccountSignatureRequest"]>): ReturnType<ClientLike["unsafeRemoveAccountSignatureRequest"]>;
unsafeRevokeAllOtherInstallationsSignatureRequest(...args: Parameters<ClientLike["unsafeRevokeAllOtherInstallationsSignatureRequest"]>): ReturnType<ClientLike["unsafeRevokeAllOtherInstallationsSignatureRequest"]>;
unsafeRevokeInstallationsSignatureRequest(...args: Parameters<ClientLike["unsafeRevokeInstallationsSignatureRequest"]>): ReturnType<ClientLike["unsafeRevokeInstallationsSignatureRequest"]>;
verifySignedWithInstallationKey(...args: Parameters<ClientLike["verifySignedWithInstallationKey"]>): ReturnType<ClientLike["verifySignedWithInstallationKey"]>;
"""
CLIENT_WORKER = """\
class Client { ... }
appVersion(...args: Parameters<ClientLike["appVersion"]>): ReturnType<ClientLike["appVersion"]>;
archives(...args: Parameters<ClientLike["archives"]>): ReturnType<ClientLike["archives"]>;
catchUpToLive(...args: Parameters<ClientLike["catchUpToLive"]>): ReturnType<ClientLike["catchUpToLive"]>;
changeRecoveryIdentifier(...args: Parameters<ClientLike["changeRecoveryIdentifier"]>): ReturnType<ClientLike["changeRecoveryIdentifier"]>;
conversations(...args: Parameters<ClientLike["conversations"]>): ReturnType<ClientLike["conversations"]>;
decodeContent(...args: Parameters<ClientLike["decodeContent"]>): ReturnType<ClientLike["decodeContent"]>;
diagnostics(...args: Parameters<ClientLike["diagnostics"]>): ReturnType<ClientLike["diagnostics"]>;
end(): Promise<void>;
events(filter: EventFilter): Promise<EventStream>;
identity(...args: Parameters<ClientLike["identity"]>): ReturnType<ClientLike["identity"]>;
inboxId(...args: Parameters<ClientLike["inboxId"]>): ReturnType<ClientLike["inboxId"]>;
inboxState(...args: Parameters<ClientLike["inboxState"]>): ReturnType<ClientLike["inboxState"]>;
installationId(...args: Parameters<ClientLike["installationId"]>): ReturnType<ClientLike["installationId"]>;
installationIdBytes(...args: Parameters<ClientLike["installationIdBytes"]>): ReturnType<ClientLike["installationIdBytes"]>;
isInMemory(...args: Parameters<ClientLike["isInMemory"]>): ReturnType<ClientLike["isInMemory"]>;
isRegistered(...args: Parameters<ClientLike["isRegistered"]>): ReturnType<ClientLike["isRegistered"]>;
latestInboxUpdatesCount(...args: Parameters<ClientLike["latestInboxUpdatesCount"]>): ReturnType<ClientLike["latestInboxUpdatesCount"]>;
libxmtpVersion(...args: Parameters<ClientLike["libxmtpVersion"]>): ReturnType<ClientLike["libxmtpVersion"]>;
options(...args: Parameters<ClientLike["options"]>): ReturnType<ClientLike["options"]>;
ownInboxUpdatesCount(...args: Parameters<ClientLike["ownInboxUpdatesCount"]>): ReturnType<ClientLike["ownInboxUpdatesCount"]>;
preferences(...args: Parameters<ClientLike["preferences"]>): ReturnType<ClientLike["preferences"]>;
protected binding(): ClientLike;
refreshServerConfiguration(...args: Parameters<ClientLike["refreshServerConfiguration"]>): ReturnType<ClientLike["refreshServerConfiguration"]>;
register(...args: Parameters<ClientLike["register"]>): ReturnType<ClientLike["register"]>;
removeAccount(...args: Parameters<ClientLike["removeAccount"]>): ReturnType<ClientLike["removeAccount"]>;
revokeAllOtherInstallations(...args: Parameters<ClientLike["revokeAllOtherInstallations"]>): ReturnType<ClientLike["revokeAllOtherInstallations"]>;
serverConfiguration(...args: Parameters<ClientLike["serverConfiguration"]>): ReturnType<ClientLike["serverConfiguration"]>;
setCredential(...args: Parameters<ClientLike["setCredential"]>): ReturnType<ClientLike["setCredential"]>;
signWithInstallationKey(...args: Parameters<ClientLike["signWithInstallationKey"]>): ReturnType<ClientLike["signWithInstallationKey"]>;
startListener(filter: EventFilter, callback: (event: ClientEvent) => void | Promise<void>): Promise<bigint>;
static build(identity: PublicIdentity, options: HostClientOptions, inboxId?: InboxId): Promise<Client>;
static canMessage(identities: PublicIdentity[], backend: BackendSource): Promise<Map<string, boolean>>;
static create(signer: Signer, options: HostClientOptions): Promise<Client>;
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
storagePath(...args: Parameters<ClientLike["storagePath"]>): ReturnType<ClientLike["storagePath"]>;
syncAllDeviceSyncGroups(...args: Parameters<ClientLike["syncAllDeviceSyncGroups"]>): ReturnType<ClientLike["syncAllDeviceSyncGroups"]>;
unsafeAddAccount(...args: Parameters<ClientLike["unsafeAddAccount"]>): ReturnType<ClientLike["unsafeAddAccount"]>;
unsafeAddAccountSignatureRequest(...args: Parameters<ClientLike["unsafeAddAccountSignatureRequest"]>): ReturnType<ClientLike["unsafeAddAccountSignatureRequest"]>;
unsafeApplySignatureRequest(...args: Parameters<ClientLike["unsafeApplySignatureRequest"]>): ReturnType<ClientLike["unsafeApplySignatureRequest"]>;
unsafeChangeRecoveryIdentifierSignatureRequest(...args: Parameters<ClientLike["unsafeChangeRecoveryIdentifierSignatureRequest"]>): ReturnType<ClientLike["unsafeChangeRecoveryIdentifierSignatureRequest"]>;
unsafeCreateInboxSignatureRequest(...args: Parameters<ClientLike["unsafeCreateInboxSignatureRequest"]>): ReturnType<ClientLike["unsafeCreateInboxSignatureRequest"]>;
unsafeRemoveAccountSignatureRequest(...args: Parameters<ClientLike["unsafeRemoveAccountSignatureRequest"]>): ReturnType<ClientLike["unsafeRemoveAccountSignatureRequest"]>;
unsafeRevokeAllOtherInstallationsSignatureRequest(...args: Parameters<ClientLike["unsafeRevokeAllOtherInstallationsSignatureRequest"]>): ReturnType<ClientLike["unsafeRevokeAllOtherInstallationsSignatureRequest"]>;
unsafeRevokeInstallationsSignatureRequest(...args: Parameters<ClientLike["unsafeRevokeInstallationsSignatureRequest"]>): ReturnType<ClientLike["unsafeRevokeInstallationsSignatureRequest"]>;
verifySignedWithInstallationKey(...args: Parameters<ClientLike["verifySignedWithInstallationKey"]>): ReturnType<ClientLike["verifySignedWithInstallationKey"]>;
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
    # Owner decision O13 (2026-09-28). Both Clients forward the exported
    # instance methods to a private binding and add static helpers and
    # callback listeners. Node's also owns codecs; the browser's uses the
    # package worker session, so apps pass no session.
    "Client": {
        NODE: CLIENT_NODE,
        WORKER: CLIENT_WORKER,
    },
    # SDK-037 adds `Storage.admin()` to the browser. The browser exports a
    # public Storage type with that factory; a Client's storage is a worker
    # proxy. Node exports the generated binding class.
    "Storage": {
        NODE: """\
class Storage extends UniffiAbstractObject implements StorageLike { ... }
delete_(asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
path(asyncOpts_?: { signal: AbortSignal; }): Promise<string | undefined>;
readonly [destructorGuardSymbol]: UniffiGcObject;
readonly [pointerLiteralSymbol]: UniffiHandle;
readonly [uniffiTypeNameSymbol] = "Storage";
reconnect(asyncOpts_?: { signal: AbortSignal; }): Promise<void>;
static instanceOf(obj_: any): obj_ is Storage;
uniffiDestroy(): void;
""",
        WORKER: """\
abstract class Storage implements StorageLike { ... }
abstract path(asyncOpts_?: { signal: AbortSignal; }): Promise<string | undefined>;
static admin(): Promise<StorageAdmin>;
""",
    },
}


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


def compare_public(out: Path) -> list[str]:
    """Compare the private public entries (`public-api.gen.ts`) of Node and the
    browser worker bridge. The browser entry omits the SDK-037 and pure-module
    exports and adds the browser storage admin; every shared declaration must
    match, member for member."""
    node_root = emit(NODE, out, "public-api.gen.ts")
    node = Surface(node_root)
    node_exports = node.exports(node.module(node_root / "public-api.gen.d.ts"))
    web_root = emit(WORKER, out, "public-api.gen.ts")
    web = Surface(web_root)
    web_exports = web.exports(web.module(web_root / "public-api.gen.d.ts"))
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
    """Compare the pure module's public entry with the Node public entry.
    Every pure export except its WASM loader is a Node public name, and each
    one must have the Node declaration."""
    node_root = emit(NODE, out, "public-api.gen.ts")
    node = Surface(node_root)
    node_exports = node.exports(node.module(node_root / "public-api.gen.d.ts"))
    pure_root = emit(PURE, out, "public-api.gen.ts")
    pure = Surface(pure_root)
    pure_exports = pure.exports(pure.module(pure_root / "public-api.gen.d.ts"))
    errors = []
    for name in sorted(PURE_PUBLIC_ONLY.keys() - pure_exports.keys()):
        errors.append(f"{name}: the pure public entry does not export it")
    shared = pure_exports.keys() - PURE_PUBLIC_ONLY.keys()
    for name in sorted(shared - node_exports.keys()):
        errors.append(
            f"{name}: the pure public entry exports it and Node does not"
        )
    for name in sorted(PURE_ONLY - pure_exports.keys()):
        errors.append(f"{name}: a pure-module name is missing from the pure entry")
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
        errors = compare(Path(out))
    with tempfile.TemporaryDirectory() as out:
        errors += compare_public(Path(out))
    with tempfile.TemporaryDirectory() as out:
        errors += compare_pure(Path(out))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("Node and browser public declarations match except the listed differences")


if __name__ == "__main__":
    main()
