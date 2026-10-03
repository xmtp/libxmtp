//! Public unions that metadata cannot express as one input.

pub(super) const BACKEND_OPTIONS: &str = r#"
export type BackendOptions = {
  readonly url: string;
  readonly appVersion?: string;
  readonly credentials?: CredentialSource | Credential;
};
export function lowerBackendOptions(value: BackendOptions, projection: ObjectProjection): B.BackendOptions {
  const credentials = value.credentials;
  return {
    url: value.url,
    appVersion: value.appVersion,
    credentials: credentials !== undefined && 'credential' in credentials ? lowerCredentialSource(credentials, projection) : undefined,
    credential: credentials !== undefined && !('credential' in credentials) ? lowerCredential(credentials, projection) : undefined,
  };
}
export function liftBackendOptions(value: B.BackendOptions, projection: ObjectProjection): BackendOptions {
  if (value.credentials !== undefined && value.credential !== undefined) throw new TypeError('multiple credential inputs');
  return {
    url: value.url,
    appVersion: value.appVersion,
    credentials: value.credentials !== undefined ? liftCredentialSource(value.credentials, projection) : value.credential !== undefined ? liftCredential(value.credential, projection) : undefined,
  };
}
"#;

pub(super) const BACKEND_SOURCE: &str = r#"
export type BackendSource = BackendOptions | Backend;
export function lowerBackendSource(value: BackendSource, projection: ObjectProjection): B.BackendSource {
  return projection.isBackend(value)
    ? B.BackendSource.Connected.new({ backend: projection.lowerBackend(value) })
    : B.BackendSource.Options.new({ options: lowerBackendOptions(value, projection) });
}
export function liftBackendSource(value: B.BackendSource, projection: ObjectProjection): BackendSource {
  switch (value.tag) {
    case B.BackendSource_Tags.Connected: return projection.liftBackend(value.inner.backend);
    case B.BackendSource_Tags.Options: return liftBackendOptions(value.inner.options, projection);
  }
}
"#;

/// A plain JavaScript caller can pass any value as a storage location. A
/// value that names no complete location fails with the public
/// `StorageLocation` error before the binding sees it.
// implements: ATCH-082
pub(super) const STORAGE_LOCATION: &str = r#"
export type StorageLocation = 'default' | 'inMemory' | { readonly directory: string } | { readonly dbPath: string; readonly attachmentsDir: string };
function storageLocationFailure(message: string): XmtpError {
  return new XmtpError.StorageLocation({ code: "StorageLocation", category: "storage", retryable: false, message });
}
function storageLocationPath(field: string, path: unknown): string {
  if (typeof path !== 'string') throw storageLocationFailure(`storage ${field} is missing`);
  if (path === '') throw storageLocationFailure(`storage ${field} is empty`);
  return path;
}
export function lowerStorageLocation(value: StorageLocation, _projection: ObjectProjection): B.StorageLocation {
  const input: unknown = value;
  if (input === 'default') return B.StorageLocation.Default.new();
  if (input === 'inMemory') return B.StorageLocation.InMemory.new();
  if (typeof input !== 'object' || input === null) throw storageLocationFailure('storage location is not default, inMemory, a directory, or explicit paths');
  if ('directory' in input) {
    if ('dbPath' in input || 'attachmentsDir' in input) throw storageLocationFailure('storage location names both a directory and explicit paths');
    return B.StorageLocation.Directory.new({ directory: storageLocationPath('directory', input.directory) });
  }
  return B.StorageLocation.Explicit.new({
    dbPath: storageLocationPath('dbPath', 'dbPath' in input ? input.dbPath : undefined),
    attachmentsDir: storageLocationPath('attachmentsDir', 'attachmentsDir' in input ? input.attachmentsDir : undefined),
  });
}
export function liftStorageLocation(value: B.StorageLocation, _projection: ObjectProjection): StorageLocation {
  switch (value.tag) {
    case B.StorageLocation_Tags.Default: return 'default';
    case B.StorageLocation_Tags.InMemory: return 'inMemory';
    case B.StorageLocation_Tags.Directory: return { directory: value.inner.directory };
    case B.StorageLocation_Tags.Explicit: return { dbPath: value.inner.dbPath, attachmentsDir: value.inner.attachmentsDir };
  }
}
"#;

/// Design section 3: a TypeScript conversation is the `Group` or `Dm` object
/// itself. Narrow it with `instanceof` or `kind()`.
pub(super) const CONVERSATION: &str = r#"
export type Conversation = Group | Dm;
export function liftConversation(value: B.Conversation, projection: ObjectProjection): Conversation {
  switch (value.tag) {
    case B.Conversation_Tags.Group: return projection.liftGroup(value.inner.group);
    case B.Conversation_Tags.Dm: return projection.liftDm(value.inner.dm);
  }
}
export function lowerConversation(value: Conversation, projection: ObjectProjection): B.Conversation {
  return value instanceof Group
    ? B.Conversation.Group.new({ group: projection.lowerGroup(value) })
    : B.Conversation.Dm.new({ dm: projection.lowerDm(value) });
}
"#;

/// Fields that the host adds to a received custom variant after its client
/// codec decodes it. The binding variant does not carry them.
pub(super) fn extra_variant_fields(owner: &str, variant: &str) -> &'static str {
    match (owner, variant) {
        ("MessageContent" | "MessageBody", "Custom") => {
            "readonly value?: unknown;\nreadonly error?: ErrorDetails;\n"
        }
        _ => "",
    }
}

/// The Delivery cursor (Ref Public surface, Delivery): an opaque replay
/// position in one database. It is a string alias, not an old-name alias.
pub(super) const DELIVERY_CURSOR: &str =
    "/** An opaque replay position in one database. */\nexport type DeliveryCursor = string;\n";

/// Record fields and results that carry a delivery cursor.
const DELIVERY_CURSOR_FIELDS: &[(&str, &str)] = &[
    ("MessageReaderOptions", "from"),
    ("ConversationMessageReaderOptions", "from"),
    ("MessageData", "deliveryCursor"),
];
const DELIVERY_CURSOR_RESULTS: &[(&str, &str)] = &[("Conversations", "beginningDeliveryCursor")];

/// The public type of a field or result: a delivery cursor is a
/// `DeliveryCursor`, not a plain `string`.
pub(super) fn cursor_type(owner: &str, member: &str, public: String) -> String {
    if DELIVERY_CURSOR_FIELDS.contains(&(owner, member))
        || DELIVERY_CURSOR_RESULTS.contains(&(owner, member))
    {
        public.replacen("string", "DeliveryCursor", 1)
    } else {
        public
    }
}

/// Check source selection before ordinary generated field conversion.
// implements: ATCH-072
pub(super) const ATTACHMENT_SOURCE_GUARD: &str = r#"
const input: unknown = value;
if (typeof input !== 'object' || input === null || !('kind' in input) ||
    !(input.kind === 'path' && 'path' in input && typeof input.path === 'string' && !('bytes' in input) ||
      input.kind === 'bytes' && 'bytes' in input && input.bytes instanceof Uint8Array && !('path' in input))) {
  throw new XmtpError.Attachment(
    { code: 'Attachment', category: 'input', retryable: false, message: 'attachment source must select exactly one path or byte buffer' },
    { cause: 'malformed', credentialKind: undefined, retryable: false, missingScope: false, httpStatus: undefined },
  );
}
"#;

// Do not let JavaScript coerce malformed foreign credentials into wire values.
// Callback rejection maps to the existing CredentialError failure in Rust.
pub(super) const CREDENTIAL_GUARD: &str = r#"
const input: unknown = value;
if (typeof input !== 'object' || input === null ||
    !('value' in input) || typeof input.value !== 'string' ||
    ('name' in input && input.name !== undefined && typeof input.name !== 'string') ||
    !('expiresAtSeconds' in input) || typeof input.expiresAtSeconds !== 'bigint' ||
    input.expiresAtSeconds < -9223372036854775808n ||
    input.expiresAtSeconds > 9223372036854775807n) {
  throw new TypeError('invalid credential record');
}
"#;
