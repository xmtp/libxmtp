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

pub(super) const STORAGE_LOCATION: &str = r#"
export type StorageLocation = 'default' | 'inMemory' | { readonly directory: string } | { readonly dbPath: string; readonly attachmentsDir: string };
export function lowerStorageLocation(value: StorageLocation, _projection: ObjectProjection): B.StorageLocation {
  if (value === 'default') return B.StorageLocation.Default.new();
  if (value === 'inMemory') return B.StorageLocation.InMemory.new();
  if ('directory' in value && ('dbPath' in value || 'attachmentsDir' in value)) throw new TypeError('multiple storage locations');
  return 'directory' in value
    ? B.StorageLocation.Directory.new({ directory: value.directory })
    : B.StorageLocation.Explicit.new({ dbPath: value.dbPath, attachmentsDir: value.attachmentsDir });
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
            "readonly value?: unknown;\nreadonly error?: string;\n"
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
