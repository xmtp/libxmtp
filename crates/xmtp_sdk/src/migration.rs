use crate::{ErrorCategory, ErrorDetails, XmtpError};

/// Closed legacy storage and a separate output path. Keep both keys private.
#[xmtp_macro::sdk_export]
#[derive(Clone, uniffi::Record)]
pub struct PrepareMigrationArchiveArgs {
    #[sdk(shown)]
    pub database_path: String,
    #[sdk(redact)]
    #[uniffi(default = None)]
    pub database_key: Option<Vec<u8>>,
    #[sdk(redact)]
    pub archive_key: Vec<u8>,
    #[sdk(shown)]
    pub output_path: String,
}

impl PrepareMigrationArchiveArgs {
    fn redacted_debug(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrepareMigrationArchiveArgs")
            .field("database_path", &self.database_path)
            .field("database_key", &"[redacted]")
            .field("archive_key", &"[redacted]")
            .field("output_path", &self.output_path)
            .finish()
    }
}

/// Counts records in the completed archive, before import deduplication.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MigrationReport {
    pub archive_path: String,
    pub group_count: u64,
    pub message_count: u64,
    pub consent_count: u64,
}

fn migration_error(error: xmtp_legacy_migration::MigrationError) -> XmtpError {
    use xmtp_legacy_migration::MigrationError;
    let details = |code: &str, message: &str| ErrorDetails {
        code: code.into(),
        category: ErrorCategory::Storage,
        retryable: false,
        message: message.into(),
        stream_failure: None,
    };
    match error {
        MigrationError::InvalidInput(_) => {
            XmtpError::invalid("invalid legacy source, key, or output path")
        }
        MigrationError::SourceBusy => XmtpError::StorageBusy(ErrorDetails {
            retryable: true,
            ..details("StorageBusy", "close the legacy SDK before migration")
        }),
        MigrationError::UnsupportedSchema => XmtpError::MigrationUnsupportedSchema(details(
            "MigrationUnsupportedSchema",
            "unsupported legacy database schema",
        )),
        MigrationError::Migration(_) => XmtpError::MigrationFailed(details(
            "MigrationFailed",
            "legacy database migration failed",
        )),
        MigrationError::RecordRead(_) => XmtpError::MigrationRecordRead(details(
            "MigrationRecordRead",
            "a required legacy record could not be read",
        )),
        MigrationError::Output(_) => XmtpError::MigrationOutput(details(
            "MigrationOutput",
            "the migration archive could not be completed",
        )),
    }
}

/// Prepare an archive without creating a client. Close the legacy SDK first.
/// Source bytes stay unchanged. Import the completed archive into the correct inbox.
#[xmtp_macro::sdk_export]
// implements: MIG-001, MIG-003
pub async fn prepare_migration_archive(
    args: PrepareMigrationArchiveArgs,
) -> Result<MigrationReport, XmtpError> {
    let report = xmtp_legacy_migration::prepare_migration_archive(
        xmtp_legacy_migration::PrepareMigrationArchiveArgs {
            database_path: args.database_path,
            database_key: args.database_key,
            archive_key: args.archive_key,
            output_path: args.output_path,
        },
    )
    .await
    .map_err(migration_error)?;
    Ok(MigrationReport {
        archive_path: report.archive_path,
        group_count: report.group_count,
        message_count: report.message_count,
        consent_count: report.consent_count,
    })
}

/// Read only a completed browser archive for the existing byte-based importer.
#[xmtp_macro::sdk_export(wasm_only)]
pub async fn read_migration_archive(archive_path: String) -> Result<Vec<u8>, XmtpError> {
    xmtp_legacy_migration::read_migration_archive(archive_path)
        .await
        .map_err(migration_error)
}
