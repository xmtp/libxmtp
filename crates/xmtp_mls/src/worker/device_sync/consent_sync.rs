use super::*;
use crate::{Client, XmtpApi};

impl<Context: XmtpSharedContext> Client<Context> {
    pub(super) fn syncable_consent_records(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let consent_records = self
            .context
            .db()
            .consent_records()?
            .into_iter()
            .map(Syncable::ConsentRecord)
            .collect();
        Ok(consent_records)
    }
}
