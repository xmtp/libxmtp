#!/usr/bin/env python3
"""Inventory the public surface of the four current SDK packages.

This is a source inventory, not a compiler ABI dump. Generated Swift sources are
counted by source family; the manifest keeps one disjoint group row per family.
Run from the repository root: python3 dev/sdk/inventory.py [--write].
"""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
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


def swift_inventory() -> list[Entry]:
    entries: list[Entry] = []
    generated = {"Proto/*.pb.swift": 0, "Libxmtp/xmtpv3.swift": 0}
    for path in sorted(SWIFT.rglob("*.swift")):
        relative = path.relative_to(SWIFT).as_posix()
        family = (
            "Proto/*.pb.swift" if relative.startswith("Proto/")
            else "Libxmtp/xmtpv3.swift" if relative == "Libxmtp/xmtpv3.swift"
            else None
        )
        depth = 0
        inherited: list[tuple[int, str, str]] = []
        for number, line in enumerate(path.read_text().splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith("//") or stripped.startswith("*"):
                continue
            while inherited and depth < inherited[-1][0]:
                inherited.pop()
            explicit = re.match(r"^(?:@[\w.]+\s+)*(?:public|open)\s+", stripped)
            parsed = compact_name(stripped, "Swift") if explicit else None
            if explicit and parsed:
                name, kind = parsed
                if family:
                    if kind != "extension":
                        generated[family] += 1
                    if kind in {"extension", "enum", "protocol"}:
                        inherited.append((depth + 1, name, kind))
                elif kind == "extension":
                    inherited.append((depth + 1, name, kind))
                else:
                    entries.append(Entry("Swift", rel(path), number, name, kind))
                    if kind in {"enum", "protocol"}:
                        inherited.append((depth + 1, name, kind))
            elif inherited and depth == inherited[-1][0]:
                owner, inherited_kind = inherited[-1][1:]
                if inherited_kind in {"extension", "protocol"}:
                    member = compact_name(stripped, "Swift")
                    if member and not stripped.startswith(("private ", "internal ", "fileprivate ")):
                        name, kind = member
                        if kind != "extension":
                            if family:
                                generated[family] += 1
                            else:
                                entries.append(Entry("Swift", rel(path), number, f"{owner}.{name}", kind))
                elif inherited_kind == "enum" and stripped.startswith("case "):
                    cases = stripped[5:].split("//")[0]
                    for case in re.split(r",\s*(?![^()]*\))", cases):
                        name = re.match(r"[A-Za-z_]\w*", case.strip())
                        if name:
                            if family:
                                generated[family] += 1
                            else:
                                entries.append(Entry("Swift", rel(path), number, f"{owner}.{name.group()}", "case"))
            # Depth is sufficient to find members in a public extension or enum.
            code = line.split("//", 1)[0]
            depth += code.count("{") - code.count("}")
    for family, count in generated.items():
        entries.append(Entry("Swift", f"sdks/ios/Sources/XMTPiOS/{family}", 0, "all public declarations", "generated family", count))
    return entries


KOTLIN_DECL = re.compile(
    r"\b(typealias|class|interface|object|fun|val|var|constructor)\b\s*(?:<[^>]+>\s*)?([A-Za-z_][\w]*)?"
)


def kotlin_inventory() -> list[Entry]:
    entries: list[Entry] = []
    for path in sorted(KOTLIN.rglob("*.kt")):
        depth = 0
        contexts: list[tuple[int, str, bool]] = []
        pending: tuple[str, bool] | None = None
        class_header = False
        in_block_comment = False
        for number, line in enumerate(path.read_text().splitlines(), 1):
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
            at_surface = not contexts or (contexts[-1][1] in {"type", "enum"} and depth == contexts[-1][0])
            hidden = bool(re.search(r"\b(private|internal|protected)\b", stripped.split("(", 1)[0]))
            match = KOTLIN_DECL.search(stripped) if stripped and not stripped.startswith("@") else None
            if match and owner_public and not hidden and (at_surface or class_header):
                kind, name = match.groups()
                name = name or ("Companion" if kind == "object" and "companion" in stripped else kind)
                if class_header and kind in {"val", "var"}:
                    kind = "constructor property"
                entries.append(Entry("Kotlin", rel(path), number, name, kind))
                if kind == "class":
                    for prop in re.finditer(r"\b(?:val|var)\s+([A-Za-z_]\w*)", stripped[match.end():]):
                        entries.append(Entry("Kotlin", rel(path), number, prop.group(1), "constructor property"))
            elif contexts and contexts[-1][1] == "enum" and depth == contexts[-1][0] and owner_public:
                variant = re.match(r"([A-Za-z_]\w*)\s*[,;(]", stripped)
                if variant:
                    entries.append(Entry("Kotlin", rel(path), number, variant.group(1), "enum case"))
            if at_surface and match and match.group(1) in {"class", "interface", "object"}:
                pending = ("enum" if "enum class" in stripped else "type", owner_public and not hidden)
                class_header = "(" in code and ")" not in code
            elif at_surface and match and match.group(1) in {"fun", "constructor"}:
                pending = ("body", owner_public and not hidden)
                class_header = False
            if class_header and ")" in code:
                class_header = False
            opens = code.count("{")
            closes = code.count("}")
            if opens:
                if pending:
                    contexts.append((depth + 1, pending[0], pending[1]))
                    pending = None
                elif not at_surface:
                    contexts.append((depth + 1, "body", owner_public))
            depth += opens - closes
            if depth < 0:
                depth = 0
    return entries


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
                entries.append(Entry(sdk, rel(path) if target else spec, line, name, "binding re-export" if not target else "re-export"))
                if target and re.search(rf"export\s+(?:abstract\s+)?class\s+{re.escape(name)}\b", target.read_text()):
                    entries.extend(ts_class_members(target, name, sdk))
        elif target:
            entries.extend(ts_exports(target, seen))
    # Direct exports from modules reached by a wildcard.
    for number, line in enumerate(text.splitlines(), 1):
        found = re.match(r"^export\s+(?:(?:declare|abstract|default)\s+)*(type|interface|class|enum|function|const|let|var)\s+([A-Za-z_]\w*)", line)
        if found:
            kind, name = found.groups()
            entries.append(Entry(sdk, rel(path), number, name, kind))
            if kind == "class":
                entries.extend(ts_class_members(path, name, sdk))
    return entries


def ts_class_members(path: Path, class_name: str, sdk: str) -> list[Entry]:
    lines = path.read_text().splitlines()
    start = next((i for i, line in enumerate(lines) if re.search(rf"\bclass\s+{re.escape(class_name)}\b", line)), None)
    if start is None:
        return []
    entries: list[Entry] = []
    depth = 0
    entered = False
    for index in range(start, len(lines)):
        line = lines[index]
        stripped = line.strip()
        if entered and depth == 1 and stripped and not stripped.startswith(("//", "*", "#", "private ", "protected ")):
            match = re.match(r"(?:(?:public|static|async|readonly|override|declare|get|set)\s+)*([A-Za-z_]\w*)\s*(?:[<(=:?]|$)", stripped)
            if match and match.group(1) not in {"return", "throw", "if", "for", "while"}:
                name = match.group(1)
                entries.append(Entry(sdk, rel(path), index + 1, f"{class_name}.{name}", "member"))
        code = line.split("//", 1)[0]
        depth += code.count("{") - code.count("}")
        if "{" in code:
            entered = True
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


REMOVED_NAMES = {
    "createInMemory", "endStream", "uploadDebugInformation",
    "streamMessageDeletions", "streamDeletedMessages", "streamConsent",
    "streamPreferenceUpdates", "streamPreferences", "fromWelcome",
    "messagesWithReactions", "enrichedMessages", "findEnrichedMessage",
    "proposalsEnabled", "isReady", "unsafe_addSignature", "encodeContent",
    "waitForRegistrationVisible", "SafeConversation",
    "toSafeConversation", "SafeSigner", "toSafeSigner", "WorkerBridge",
    "WorkerAuth", "createEOASigner", "createSCWSigner", "Topic", "KeyUtil",
    "PrivateKeyBuilder", "Crypto", "EncodedContentCompression",
    "DecodedMessageV2", "ClientError", "ConversationError", "XMTPException",
    "StreamFailedError", "StreamInvalidRetryAttemptsError",
    "GroupNotFoundError", "StreamNotFoundError", "OpfsNotInitializedError",
    "OpfsInitializationError", "DEFAULT_RETRY_DELAY", "DEFAULT_RETRY_ATTEMPTS",
    "getStreamFailureDetails", "getErrorCode", "HexString", "isHexString",
    "validHex", "createStream", "PreEventCallback", "VisibilityConfirmationOptions",
    "EntryType", "PreferenceType", "encryptAttachment", "decryptAttachment",
    "toFfi", "fromFfi", "toFfiPublicIdentifierKind", "childMessages",
    "Opfs",
}
RENAMES = {
    "DecodedMessage": "Message", "PrivatePreferences": "Preferences",
    "findConversation": "getByID()", "findGroup": "getByID()",
    "findConversationByTopic": "getByID()", "getConversationById": "getByID()",
    "findDmByInboxId": "getDmByInboxID()", "findDmByIdentity": "getDmByIdentity()",
    "findMessage": "getMessageByID()", "getDebugInformation": "debugInfo()",
    "debugInformation": "diagnostics", "leaveGroup": "requestRemoval()",
    "syncAllConversations": "syncAll()", "newGroup": "createGroup()",
    "newConversation": "createDm()", "getHmacKeys": "hmacKeys()",
    "getLastReadTimes": "lastReadTimes()", "streamMessages": "stream()",
    "inboxId": "inboxID", "installationId": "installationID",
    "peerInboxId": "peerInboxID", "publicIdentity": "identity",
    "libXMTPVersion": "libxmtpVersion", "dbPath": "storage.path",
    "metadata": "kind / creatorInboxID", "numReplies": "replyCount",
    "connectToApiBackend": "Backend.connect()", "createBackend": "Backend.connect()",
    "getOrCreateInboxId": "Client.inboxID()", "generateInboxId": "Client.inboxID()",
    "getInboxIdForIdentifier": "Client.inboxID()",
    "updateMessageDisappearingSettings": "updateDisappearingSettings()",
    "removeMessageDisappearingSettings": "updateDisappearingSettings(null)",
    "createGroupWithIdentifiers": "createGroupWithIdentities()",
    "createDmWithIdentifier": "createDmWithIdentity()",
    "fetchDmByIdentifier": "getDmByIdentity()",
    "Api": "BackendOptions",
    "SigningKey": "Signer", "SignedData": "Signature", "SignerType": "SignerKind",
    "api": "backend", "backendUrl": "backend.url", "env": "storage.label",
    "dbDirectory": "storage.location.directory", "dbEncryptionKey": "storage.encryptionKey",
    "deviceSyncEnabled": "deviceSync", "forkRecoveryOptions": "forkRecovery",
    "dbPoolOptions": "storage.pool", "preAuthenticateToInboxCallback": "handlers.preAuthenticate",
    "authCallback": "backend.credentials", "environment": "options.storage.label",
    "createGroupWithIdentities": "createGroupWithIdentities()",
    "ffiCreateClient": "Client.build()", "ffiApplySignatureRequest": "unsafeApplySignatureRequest()",
    "ffiRevokeInstallations": "unsafeRevokeInstallationsSignatureRequest()",
    "ffiRevokeAllOtherInstallations": "unsafeRevokeAllOtherInstallationsSignatureRequest()",
    "ffiRevokeIdentity": "unsafeRemoveAccountSignatureRequest()",
    "ffiAddIdentity": "unsafeAddAccountSignatureRequest()",
    "ffiSignatureRequest": "unsafeCreateInboxSignatureRequest()",
    "ffiRegisterIdentity": "register()",
    "Identifier": "PublicIdentity", "Consent": "ConsentRecord",
    "SendOpts": "SendOptions", "SendMessageOpts": "SendOptions",
    "ConversationFilterType": "ConversationKind", "ConversationsOrderBy": "ConversationOrder",
    "ConversationType": "ConversationKind", "GroupMessageKind": "MessageKind",
    "DebugInformation": "Diagnostics",
    "deleteLocalDatabase": "storage.delete()",
    "dropLocalDatabaseConnection": "end()",
    "reconnectLocalDatabase": "storage.reconnect()",
    "createArchive": "archives.exportToFile()",
    "importArchive": "archives.importFromFile()",
    "archiveMetadata": "archives.metadataFromFile()",
}


def final_spelling(name: str) -> str:
    if name.endswith("AtNs") and not name.endswith("lastActivityAtNs"):
        name = name[:-4] + "At.ns"
    name = re.sub(r"Id\b", "ID", name)
    name = re.sub(r"Ids\b", "IDs", name)
    name = re.sub(r"^unsafe_([a-z])", lambda m: "unsafe" + m.group(1).upper(), name)
    return name


def classify(entry: Entry) -> tuple[str, str, str, bool]:
    path, name = entry.source, entry.name
    leaf = name.split(".")[-1]
    if leaf == "register" and entry.sdk in {"Swift", "Kotlin"} and "/Client." in path:
        return "approved removal", "—", "Global codec registration becomes ClientOptions.codecs (11.4, 19.9).", False
    if leaf == "dbEncryptionKey" and entry.sdk == "Browser":
        return "approved removal", "—", "The browser never used this option (11.4 Browser, 19.25).", False
    if leaf == "codecRegistry":
        if entry.sdk in {"Swift", "Kotlin"}:
            return "approved removal", "—", "Global registry becomes per-client codecs (11.4, 19.9).", False
        return "static runtime", final_spelling(name), "Per-client codec registry stays in the host runtime (11.4).", False
    if leaf in {"SigningKey", "SignedData", "SignerType"}:
        return "generated", final_spelling(RENAMES[leaf]), "Signer contract changes shape (11.1, 11.4).", False
    if leaf == "metadata" and any(part in path for part in ("/Conversation.", "/Group.", "/Dm.")):
        return "approved removal", "—", "Immutable kind and creatorInboxID replace metadata() (11.4).", False
    if entry.sdk == "Kotlin" and entry.kind in {"val", "var"} and any(part in path for part in ("/Conversation.kt", "/Group.kt", "/Dm.kt")):
        source_lines = (ROOT / path).read_text().splitlines()
        if any("@Deprecated" in prior for prior in source_lines[max(0, entry.line - 8):entry.line - 1]):
            return "approved removal", "—", "Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4).", False
    if entry.kind == "generated family":
        if "Proto/" in path:
            return "approved removal", "—", "Generated SwiftProtobuf files leave the package (2, 19.34).", False
        return "generated", "facade-generated bindings", "Old UniFFI output is replaced from the facade (2, 19.45).", False
    if any(part in REMOVED_NAMES for part in name.split(".")) or "Unstable" in path or "Topic." in name:
        return "approved removal", "—", "Removal or replacement approved in 11.4 and 19.", False
    if leaf in {"debugEventsEnabled", "unstableChangeCallbacks"}:
        return "approved removal", "—", "Debug events or unstable callbacks leave ClientOptions (11.4, 19.31/35).", False
    if leaf.startswith("ffi") and leaf in RENAMES:
        return "alias", final_spelling(RENAMES[leaf]), "Delicate flow takes the canonical unsafe name (11.4, 19.24).", False
    if leaf.startswith("ffi"):
        return "approved removal", "—", "Old binding helper leaves the public API (11.4).", False
    if leaf == "close" and any(part in path for part in ("/Client.", "/MessageReader.", "/EventReader.")):
        return "alias", "end()", "Async client and reader shutdown uses end() (plan Decisions, Section 20.8).", False
    if leaf in {"latestInboxUpdatesCount", "keyPackageStatuses", "newestMessageMetadata", "hmacKeys", "lastReadTimes"}:
        return "generated", final_spelling(name), "Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7).", False
    if leaf == "lastActivityAtNs":
        return "generated", "lastActivityAtNs(contentTypes?)", "Optional content-type filter; outside state() (plan Decisions, 19.46).", False
    if leaf == "lastActivityNs":
        return "generated", "lastActivityAtNs(contentTypes?)", "Optional content-type filter; outside state() (plan Decisions, 19.46).", False
    if any(part in path for part in ("/Conversation.", "/Group.", "/Dm.")) and leaf in {
        "isActive", "consentState", "pausedForVersion", "isDisappearingMessagesEnabled",
        "isMessageDisappearingEnabled", "disappearingMessageSettings", "messageDisappearingSettings",
        "membershipState", "commitLogForkStatus", "notificationsEnabled", "name", "imageUrl",
        "description", "appData", "admins", "superAdmins", "permissions", "permissionPolicySet",
    }:
        field = {"isDisappearingMessagesEnabled": "isDisappearingEnabled", "isMessageDisappearingEnabled": "isDisappearingEnabled", "disappearingMessageSettings": "disappearingSettings", "messageDisappearingSettings": "disappearingSettings", "permissionPolicySet": "permissions.policySet"}.get(leaf, leaf)
        return "generated", f"state().{field}", "One conversation-state read (11.2, 11.4, 19.4).", False
    if entry.sdk == "Browser" and leaf in {"createArchive", "importArchive", "archiveMetadata"}:
        final = {"createArchive": "archives.exportToBytes()", "importArchive": "archives.importFromBytes()", "archiveMetadata": "archives.metadataFromBytes()"}[leaf]
        return "alias", final, "Browser archives use bytes (11.4 Browser, 19.25).", False
    if leaf in {"ClientOptions", "codecs", "preAuthenticateToInboxCallback", "appContext"} and any(part in path for part in ("/Client.", "/types.ts", "/types/options.ts")):
        final = RENAMES.get(leaf, leaf)
        if leaf == "appContext":
            return "platform helper", "StorageOptions(context)", "Android Context overload stays native (2, 19.26).", False
        return "static runtime", final_spelling(final), "Host options wrapper accepts codecs or callbacks (11.1, 20.1).", False
    if entry.sdk in {"Node", "Browser"} and name in {"Client.create", "Client.build", "Client.decodeContent"}:
        return "static runtime", final_spelling(name), "Host wrapper owns codecs and the client registry (11.1, 11.7).", False
    if "DecodedMessageV2" in path and leaf not in {"Intent", "Actions"}:
        if leaf in {"body", "create", "childMessages"}:
            return "approved removal", "—", "Merged into the Message value model (11.4, 19.5).", False
        return "static runtime", "Message." + final_spelling(leaf), "Old live getter moves to the Message host value (11.7).", False
    if entry.sdk in {"Swift", "Kotlin"}:
        if any(part in path for part in ("StreamLifecycle", "XMTPLogger", "Extensions/URL")):
            return "platform helper", final_spelling(name), "Native OS integration stays under sdks/ (2).", False
        if entry.sdk == "Swift" and "/Extensions/" in path:
            return "static runtime", final_spelling(name), "Proposed host helper; the design does not name this extension export.", True
        if "RemoteAttachmentCodec" in path and leaf in {"content", "load", "loadRemoteAttachment"}:
            return "platform helper", "RemoteAttachmentDownload", "Native HTTPS download stays under sdks/ (2, 11.4).", False
        if leaf in {"manageStreamLifecycle", "activatePersistentLibXMTPLogWriter", "deactivatePersistentLibXMTPLogWriter"}:
            return "platform helper", final_spelling(name), "Native lifecycle or log writer helper (2, 19.26/35).", False
        if any(part in path for part in ("Codecs/", "codecs/", "DecodedMessage", "CodecRegistry", "MessageReader", "MessageDelivery", "PrivatePreferences")):
            if leaf in RENAMES:
                return "alias", final_spelling(RENAMES[leaf]), "Deprecated name for one major release (19.2; 11.4).", False
            return "static runtime", final_spelling(name), "Host message, codec, preference, or stream runtime (2, 11.7).", False
        if any(part in path for part in ("KeyUtil", "Crypto.", "Messages/PrivateKey", "messages/PrivateKey", "Util.")):
            return "approved removal", "—", "Replaced by Rust signer or encryption (11.4, 19.32/34).", False
    else:
        if entry.sdk == "Browser" and leaf == "metadataFieldName":
            return "static runtime", "metadataFieldName", "Proposed host helper; the design does not name this export.", True
        if leaf == "DecodedMessage":
            return "alias", "Message", "Deprecated alias for one major release (11.4, 19.5).", False
        if leaf in {"CodecRegistry", "MessageStream", "AsyncStreamProxy", "ResolveValue", "MessageAcknowledgement", "MessageDelivery", "MessageReaderSource"}:
            return "static runtime", final_spelling(name), "Host codec or stream adapter (2, 5, 11.7).", False
        if entry.kind == "binding re-export":
            return "generated", final_spelling(name), "Binding export supplied by the facade generator (11.4).", False
        if any(part in path for part in ("CodecRegistry", "DecodedMessage", "MessageStream", "AsyncStream", "/utils/contentTypes", "/utils/messages", "/utils/signer", "/types")):
            if leaf == "DecodedMessage":
                return "alias", "Message", "Deprecated alias for one major release (11.4, 19.5).", False
            return "static runtime", final_spelling(name), "Host class, codec, stream, or option type (2, 11.7).", False
        if any(part in path for part in ("/utils/conversions", "/utils/Worker", "/Opfs")):
            return "approved removal", "—", "Browser transport or Safe* type is replaced (11.4 Browser, 19.39).", False
        if "/utils/errors" in path:
            return "generated", final_spelling(name), "Generated XmtpError variant or helper (11.1, 11.4).", False
        if "/utils/streamFailure" in path:
            return "generated", final_spelling(name), "Typed stream failure details (5, 11.4).", False
        if "/utils/streams" in path:
            return "static runtime", final_spelling(name), "Host stream adapter and options (5, 11.4).", False
        if "/utils/" in path and leaf not in RENAMES:
            return "static runtime", final_spelling(name), "Proposed host utility; design does not name this export.", True
    if leaf in RENAMES:
        return "alias", final_spelling(RENAMES[leaf]), "Deprecated rename for one major release (11.4, 19.2).", False
    if "Notification" in path and entry.sdk == "Browser":
        return "approved removal", "—", "Browser notifications remain absent (19.25).", False
    if entry.kind in {"extension", "case"}:
        return "generated", final_spelling(name), "Enum case or generated value (11.1-11.2).", False
    return "generated", final_spelling(name), "Facade schema or generated record (11.1-11.4).", False


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
        "The [design Ref](https://plan.ref.tools/eG4NJ6emCjsHcWH0), especially Sections 11 and 19, is the authority. "
        "The implementation plan adopts Section 20 items 1, 2, 3, 4, and 7, and uses `end()` for async shutdown. "
        "Final names use stock generator spelling: `ID` suffixes, `unsafe` camel case, string IDs, and one `Timestamp` value with `.ns` and `.date`.", "",
        "`generated` means the facade generator emits the API. `static runtime` means hand-written host code ships with generated output. "
        "`platform helper` means native OS code stays in the SDK. `alias` means a deprecated compatibility name. "
        "`approved removal` means the current export leaves the API. A dash in Final name marks a removal. "
        "Each table has one row per inventory entry; a generated Swift source-family row covers the stated number of declarations. "
        "A source path and line number distinguish overloads. Kind names the current declaration form. "
        "The helper counts source-declared Swift public/open items, Kotlin public declarations and constructor properties, "
        "and TypeScript package exports plus exported class members. Compiler-synthesized members are outside this source inventory. "
        "The counts are declaration counts, not table-row counts. Run `python3 dev/sdk/inventory.py --check` to recompute them.", "",
    ]
    counts = {sdk: sum(e.count for e in entries) for sdk, entries in inventories.items()}
    lines += ["| SDK | Public declarations |", "| --- | ---: |"]
    lines += [f"| {sdk} | {count} |" for sdk, count in counts.items()]
    lines += [""]
    open_items: list[str] = []
    for sdk, entries in inventories.items():
        lines += [f"## {sdk}", "", "| Current export | Kind | Final name | Status | Design ref | Notes |", "| --- | --- | --- | --- | --- | --- |"]
        seen: set[str] = set()
        for entry in sorted(entries, key=lambda e: (e.source, e.line, e.name)):
            if entry.key in seen:
                raise ValueError(f"duplicate inventory key: {entry.key}")
            seen.add(entry.key)
            status, final, note, is_open = classify(entry)
            if status not in {"generated", "static runtime", "platform helper", "alias", "approved removal"}:
                raise ValueError(status)
            source_ref = "11.4 " + sdk if not is_open else "2; open"
            if entry.kind == "generated family":
                source_ref = "2; 19.34" if "Proto/" in entry.source else "2; 11.4 Swift"
            elif status == "platform helper":
                source_ref = "2"
            elif status == "alias":
                source_ref = "11.4 " + sdk + "; 19.2"
            current = f"`{entry.key}`"
            if entry.count > 1:
                current += f" ({entry.count} declarations)"
            lines.append("| " + " | ".join(markdown_cell(v) for v in (current, entry.kind, f"`{final}`" if final != "—" else final, status, source_ref, note)) + " |")
            if is_open:
                open_items.append(f"- {sdk} `{entry.key}`: proposed **{status}**. The design does not name this utility export.")
        lines.append("")
    lines += ["## Open items", ""]
    if open_items:
        lines += [f"{len(open_items)} exports need a design decision. Their proposed status appears in the SDK table.", ""]
        lines += open_items
    else:
        lines.append("None in the source inventory above.")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
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
