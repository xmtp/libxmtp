//! Typed metadata fields and user data.
//!
//! These methods are the developer-facing view of a group's AppData
//! dictionary. Each call reads one committed dictionary snapshot through
//! [`FieldSnapshot`]; the committed registry decides which fields exist and
//! their policies, the protocol fixes a well-known field's type, and the backend
//! catalogue only labels application fields. Writes are checked against the
//! committed registry's types and policies before they are queued as
//! [`AppDataUpdateIntentData::Fields`], and again when the commit is built,
//! so a write every receiver would reject is never published.

use std::collections::BTreeMap;

use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_mls_common::{
    app_data::fields::{
        ComponentMutation, FieldError, FieldKey, FieldSnapshot, FieldValue, FieldWrite,
        MetadataFieldDescriptor, MetadataFieldRef, MetadataFieldValue, MetadataValue,
        UserFieldUpdate, UserFieldValue,
    },
    inbox_id::InboxId,
};

use super::{
    GroupError, MlsGroup,
    app_data::sender_intents::resolve_field_writes,
    intents::{AppDataUpdateIntentData, QueueIntent},
};
use crate::context::XmtpSharedContext;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// The group's metadata fields in component ID order.
    // implements: META-069
    pub fn metadata_fields(&self) -> Result<Vec<MetadataFieldDescriptor>, GroupError> {
        self.read_fields(|fields| Ok(fields.fields().to_vec()))
    }

    /// The field named `name`, preferring a well-known field to an
    /// application field of the same name.
    // implements: META-069
    pub fn metadata_field(
        &self,
        name: &str,
    ) -> Result<Option<MetadataFieldDescriptor>, GroupError> {
        self.read_fields(|fields| Ok(fields.field(name).cloned()))
    }

    /// The value of `field`, `None` when absent.
    // implements: META-070
    pub fn metadata_value(
        &self,
        field: &MetadataFieldRef,
    ) -> Result<Option<MetadataValue>, GroupError> {
        self.read_fields(|fields| fields.value(field))
    }

    /// The values of `fields` in request order, read from one snapshot.
    // implements: META-070
    pub fn metadata_values(
        &self,
        fields: &[MetadataFieldRef],
    ) -> Result<Vec<MetadataFieldValue>, GroupError> {
        self.read_fields(|snapshot| snapshot.values(fields))
    }

    /// The value under `key` of the map `field`.
    // implements: META-070
    pub fn map_value(
        &self,
        field: &MetadataFieldRef,
        key: &FieldKey,
    ) -> Result<Option<FieldValue>, GroupError> {
        self.read_fields(|fields| fields.map_value(field, key))
    }

    /// Per-inbox user field values, read from one snapshot. See
    /// [`FieldSnapshot::user_data`] for the `None` selections.
    // implements: META-072
    pub fn user_data(
        &self,
        fields: Option<&[MetadataFieldRef]>,
        inbox_ids: Option<&[InboxId]>,
    ) -> Result<BTreeMap<InboxId, Vec<UserFieldValue>>, GroupError> {
        self.read_fields(|snapshot| snapshot.user_data(fields, inbox_ids))
    }

    /// Apply `mutation` to `field` in one commit, subject to the committed
    /// registry's type and policies.
    // implements: META-071
    pub async fn update_metadata_field(
        &self,
        field: &MetadataFieldRef,
        mutation: &ComponentMutation,
    ) -> Result<(), GroupError> {
        self.write_fields(|fields| Ok(vec![fields.field_write(field, mutation)?]))
            .await
    }

    /// Set or clear this inbox's own entries of user fields in one commit.
    /// Nothing is committed when `values` is empty or only clears absent
    /// entries.
    // implements: META-073
    pub async fn update_user_data(&self, values: &[UserFieldUpdate]) -> Result<(), GroupError> {
        self.write_fields(|fields| fields.user_data_writes(values))
            .await
    }

    fn read_fields<T>(
        &self,
        read: impl FnOnce(&FieldSnapshot<'_>) -> Result<T, FieldError>,
    ) -> Result<T, GroupError> {
        let context = self.load_group_context()?;
        let dictionary = context
            .extensions()
            .app_data_dictionary()
            .map(|extension| extension.dictionary());
        let catalogue = &self
            .context
            .server_configuration()
            .configuration()
            .application_components;
        Ok(read(&FieldSnapshot::new(dictionary, catalogue)?)?)
    }

    /// Encode writes with `plan` and commit them. The writes are resolved
    /// and authorized once here, so a type error, a denied policy, or a
    /// payload the group would reject is reported to the caller rather than
    /// as a failed intent. The group is synced first, so a write that
    /// changes nothing, which commits nothing, is judged against the
    /// group's state during this call rather than a stale local copy.
    async fn write_fields(
        &self,
        plan: impl FnOnce(&FieldSnapshot<'_>) -> Result<Vec<FieldWrite>, FieldError>,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        self.sync().await?;
        let own = InboxId::from_hex(self.context.inbox_id())
            .map_err(|e| GroupError::ComponentSource(e.into()))?;
        let writes = self.with_group_snapshot(|group: &OpenMlsGroup| {
            let committed = group
                .extensions()
                .app_data_dictionary()
                .map(|extension| extension.dictionary());
            let writes = plan(&FieldSnapshot::new(committed, &[])?)?;
            let changes = !resolve_field_writes(group, own, &writes)?.is_empty();
            Ok(changes.then_some(writes))
        })?;
        let Some(writes) = writes else {
            return Ok(());
        };
        let intent = QueueIntent::app_data_update()
            .data(Vec::<u8>::from(AppDataUpdateIntentData::Fields(writes)))
            .queue(self)?;
        self.sync_until_intent_resolved(intent.id).await.map(drop)
    }
}
