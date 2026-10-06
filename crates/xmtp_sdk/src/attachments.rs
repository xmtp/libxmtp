//! Attachment files owned by a client.
//!
//! The SDK stages the data of a new attachment when the app creates it, and
//! uploads or downloads only when the app asks. It does not retry a failed
//! transfer: the app calls the operation again. The SDK does not make image
//! previews; the app makes them from the local file.

use std::{path::PathBuf, sync::Arc};

use xmtp_mls::attachments::{
    AttachmentSource as CoreSource, PendingAttachment as CorePending,
    PendingAttachmentStatus as CoreStatus,
};

use crate::{
    AttachmentFailure, RemoteAttachment, Timestamp, XmtpError,
    client::CoreClient,
    conversation::{enter_call, on_settled_worker},
};

type CoreRemote = xmtp_content_types::remote_attachment::RemoteAttachment;

/// The data of a new attachment. The SDK copies it into its own storage at
/// create, so later changes to the source do not change the attachment.
///
/// In the browser, `path` names a file in the SDK's OPFS storage pool, and the
/// SDK copies `bytes` to its worker; the caller keeps its buffer.
///
/// Every host passes `bytes` by copy, so a source holds its size in memory at
/// least twice until create returns. Prefer `path` for data over 1 MiB: the
/// SDK reads a path source in small chunks.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum AttachmentSource {
    Path {
        path: String,
        filename: Option<String>,
        mime_type: String,
    },
    Bytes {
        bytes: Vec<u8>,
        filename: Option<String>,
        mime_type: String,
    },
}

impl From<AttachmentSource> for CoreSource {
    fn from(value: AttachmentSource) -> Self {
        match value {
            AttachmentSource::Path {
                path,
                filename,
                mime_type,
            } => Self::Path {
                path: PathBuf::from(path),
                filename,
                mime_type,
            },
            AttachmentSource::Bytes {
                bytes,
                filename,
                mime_type,
            } => Self::Bytes {
                bytes,
                filename,
                mime_type,
            },
        }
    }
}

/// The upload state of a pending attachment, read from the database.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum PendingAttachmentStatus {
    Waiting,
    Uploading,
    Complete,
    Failed(AttachmentFailure),
}

impl From<CoreStatus> for PendingAttachmentStatus {
    fn from(value: CoreStatus) -> Self {
        match value {
            CoreStatus::Waiting => Self::Waiting,
            CoreStatus::Uploading => Self::Uploading,
            CoreStatus::Complete => Self::Complete,
            CoreStatus::Failed(error) => Self::Failed(error.into()),
        }
    }
}

/// A verified attachment in the attachments directory.
#[derive(Clone, Debug, uniffi::Record)]
pub struct DownloadedAttachment {
    pub path: String,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}

/// A plaintext attachment file this client keeps.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LocalAttachment {
    /// The file's path inside the attachments directory, `{key}/{file}`.
    pub path: String,
    pub created_at: Timestamp,
}

fn path_string(path: PathBuf) -> Result<String, XmtpError> {
    path.into_os_string().into_string().map_err(|_| {
        XmtpError::attachment(AttachmentFailure::with_cause(
            crate::AttachmentFailureCause::LocalStorage,
        ))
    })
}

/// The attachments of one client. The paths it returns name files under the
/// client's attachments directory; in the browser they name OPFS entries.
///
/// The SDK stores and transfers the file an app gives it. It makes no preview
/// or thumbnail: an app that shows an image makes its own from the local file.
/// The SDK retries no failed upload or download. A thrown
/// `XmtpError.Attachment` says in its `retryable` detail whether the same call
/// can succeed later. A `Failed` status has no such detail: its failure
/// decides it by the ATCH cause table, from `cause`, from `httpStatus` for the
/// `HttpStatus` cause, and from `retryable` for the `Credential` cause.
#[derive(uniffi::Object)]
pub struct Attachments {
    pub(crate) client: Arc<CoreClient>,
}

impl Attachments {
    fn pending_object(&self, inner: CorePending<xmtp_mls::MlsContext>) -> Arc<PendingAttachment> {
        Arc::new(PendingAttachment {
            client: self.client.clone(),
            inner,
        })
    }
}

#[xmtp_macro::sdk_export]
impl Attachments {
    /// Whether the deployment's server configuration offers attachments.
    #[sdk(immutable)]
    pub fn offered(&self) -> bool {
        self.client.attachments().offered()
    }

    /// The path of the attachment's plaintext file. The file exists only
    /// after a download or a create on this client.
    pub async fn local_path(&self, remote: RemoteAttachment) -> Result<String, XmtpError> {
        let _call = enter_call(&self.client.context)?;
        let remote = CoreRemote::from(remote);
        self.client
            .attachments()
            .local_path(&remote)
            .map_err(XmtpError::from_attachment)
            .and_then(path_string)
    }

    /// Stage a new attachment and return it before any request. Send its
    /// remote attachment, then upload it.
    pub async fn create(
        &self,
        source: AttachmentSource,
    ) -> Result<Arc<PendingAttachment>, XmtpError> {
        let client = self.client.clone();
        let pending = on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .attachments()
                    .create(source.into())
                    .await
                    .map_err(XmtpError::from_attachment)
            }),
        )
        .await?;
        Ok(self.pending_object(pending))
    }

    /// The pending attachment for this remote attachment.
    pub async fn pending(
        &self,
        remote: RemoteAttachment,
    ) -> Result<Arc<PendingAttachment>, XmtpError> {
        let client = self.client.clone();
        let remote = CoreRemote::from(remote);
        let pending = on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .attachments()
                    .pending(&remote)
                    .await
                    .map_err(XmtpError::from_attachment)
            }),
        )
        .await?;
        Ok(self.pending_object(pending))
    }

    /// Every pending attachment that has not completed its upload.
    pub async fn list_pending(&self) -> Result<Vec<Arc<PendingAttachment>>, XmtpError> {
        let client = self.client.clone();
        let pending = on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .attachments()
                    .list_pending()
                    .await
                    .map_err(XmtpError::from_attachment)
            }),
        )
        .await?;
        Ok(pending
            .into_iter()
            .map(|pending| self.pending_object(pending))
            .collect())
    }

    /// Download, verify, and decrypt the attachment into the attachments
    /// directory, or return the file a completed download left there. A
    /// failed download is not retried.
    pub async fn download(
        &self,
        remote: RemoteAttachment,
    ) -> Result<DownloadedAttachment, XmtpError> {
        let client = self.client.clone();
        let remote = CoreRemote::from(remote);
        let downloaded = on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .attachments()
                    .download(&remote)
                    .await
                    .map_err(XmtpError::from_attachment)
            }),
        )
        .await?;
        Ok(DownloadedAttachment {
            path: path_string(downloaded.path)?,
            mime_type: downloaded.mime_type,
            filename: downloaded.filename,
        })
    }

    /// Delete the attachment's local files and records.
    pub async fn delete_local(&self, remote: RemoteAttachment) -> Result<(), XmtpError> {
        let client = self.client.clone();
        let remote = CoreRemote::from(remote);
        on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .attachments()
                    .delete_local(&remote)
                    .await
                    .map_err(XmtpError::from_attachment)
            }),
        )
        .await
    }

    /// The plaintext attachment files this client keeps.
    pub async fn list_local(&self) -> Result<Vec<LocalAttachment>, XmtpError> {
        let client = self.client.clone();
        let local = on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .attachments()
                    .list_local()
                    .await
                    .map_err(XmtpError::from_attachment)
            }),
        )
        .await?;
        Ok(local
            .into_iter()
            .map(|local| LocalAttachment {
                path: local.path,
                created_at: Timestamp(local.created_at_ns),
            })
            .collect())
    }
}

/// A staged attachment and its upload.
#[derive(uniffi::Object)]
pub struct PendingAttachment {
    client: Arc<CoreClient>,
    inner: CorePending<xmtp_mls::MlsContext>,
}

#[xmtp_macro::sdk_export]
impl PendingAttachment {
    /// The remote attachment to send. It is complete before the upload.
    #[sdk(immutable)]
    pub fn remote_attachment(&self) -> RemoteAttachment {
        self.inner.remote_attachment().clone().into()
    }

    /// The path of the attachment's plaintext file.
    pub async fn local_path(&self) -> Result<String, XmtpError> {
        let _call = enter_call(&self.client.context)?;
        self.inner
            .local_path()
            .map_err(XmtpError::from_attachment)
            .and_then(path_string)
    }

    pub async fn status(&self) -> Result<PendingAttachmentStatus, XmtpError> {
        let inner = self.inner.clone();
        on_settled_worker(self.client.context.clone(), async move {
            Ok(inner.status().into())
        })
        .await
    }

    /// Upload the staged ciphertext. Concurrent calls share one upload. A
    /// failed upload stays failed until the app calls upload again; after a
    /// `backend_rejected` failure, upload fails again without a request.
    pub async fn upload(&self) -> Result<(), XmtpError> {
        let inner = self.inner.clone();
        on_settled_worker(
            self.client.context.clone(),
            Box::pin(async move { inner.upload().await.map_err(XmtpError::from_attachment) }),
        )
        .await
    }
}

#[cfg(feature = "conformance")]
#[xmtp_macro::sdk_export]
impl PendingAttachment {
    /// Record a terminal failed upload so a host can read every failure
    /// field back from `status()`.
    pub async fn sdk_conformance_fail(&self, failure: AttachmentFailure) -> Result<(), XmtpError> {
        use xmtp_db::attachments::{PendingAttachmentOutcome, QueryPendingAttachment as _};
        let context = self.client.context.clone();
        let digest = self.inner.remote_attachment().content_digest.clone();
        on_settled_worker(self.client.context.clone(), async move {
            let error = xmtp_mls::attachments::AttachmentClientError::from(failure);
            let lease = xmtp_common::rand_array::<16>();
            let now = xmtp_common::time::now_ns();
            let db = context.db();
            let claimed = db
                .claim_pending_attachment(&digest, &lease, now, 60_000_000_000)
                .map_err(XmtpError::unknown)?;
            if claimed != 1 {
                return Err(XmtpError::unknown("pending attachment is not claimable"));
            }
            db.finish_pending_attachment(
                &digest,
                &lease,
                now,
                PendingAttachmentOutcome::Failed {
                    cause: error.cause.as_str(),
                    credential_kind: error.credential_kind.map(|kind| kind.as_str()),
                    retryable: Some(error.retryable),
                    missing_scope: Some(error.missing_scope),
                    http_status: error.http_status,
                },
            )
            .map_err(XmtpError::unknown)?;
            Ok(())
        })
        .await
    }
}

/// Throw the attachment error that carries this failure, so a host can
/// check its thrown form.
#[cfg(feature = "conformance")]
#[xmtp_macro::sdk_export]
pub async fn sdk_conformance_attachment_error(failure: AttachmentFailure) -> Result<(), XmtpError> {
    Err(XmtpError::attachment(failure))
}
