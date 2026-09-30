// Typed metadata fields and user data for Group and Dm. Each method passes
// the whole request to one core call, which reads one committed snapshot or
// makes at most one commit.
macro_rules! metadata_conversation {
    ($name:ident) => {
        #[xmtp_macro::sdk_export]
        impl $name {
            /// The conversation's fields in component ID order.
            // implements: META-069
            pub async fn metadata_fields(
                &self,
            ) -> Result<Vec<crate::MetadataFieldDescriptor>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    let fields = group.metadata_fields().map_err(XmtpError::from_group)?;
                    Ok(fields.into_iter().map(Into::into).collect())
                })
                .await
            }

            /// The field named `name`. A well-known field wins over an
            /// application field with the same name.
            // implements: META-069
            pub async fn metadata_field(
                &self,
                name: String,
            ) -> Result<Option<crate::MetadataFieldDescriptor>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    let field = group.metadata_field(&name).map_err(XmtpError::from_group)?;
                    Ok(field.map(Into::into))
                })
                .await
            }

            /// The value of `field`, absent when the conversation holds none.
            // implements: META-070
            pub async fn metadata_value(
                &self,
                field: crate::MetadataFieldRef,
            ) -> Result<Option<crate::MetadataValue>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    let value = group
                        .metadata_value(&field.into())
                        .map_err(XmtpError::from_group)?;
                    Ok(value.map(Into::into))
                })
                .await
            }

            /// The values of `fields` in request order, read from one
            /// snapshot.
            // implements: META-070
            pub async fn metadata_values(
                &self,
                fields: Vec<crate::MetadataFieldRef>,
            ) -> Result<Vec<crate::MetadataFieldValue>, XmtpError> {
                let group = self.inner.clone();
                let fields: Vec<_> = fields.into_iter().map(Into::into).collect();
                on_sdk_worker(self.inner.context.clone(), async move {
                    let values = group
                        .metadata_values(&fields)
                        .map_err(XmtpError::from_group)?;
                    Ok(values.into_iter().map(Into::into).collect())
                })
                .await
            }

            /// The value under `key` of the map `field`.
            // implements: META-070
            pub async fn map_value(
                &self,
                field: crate::MetadataFieldRef,
                key: crate::FieldKey,
            ) -> Result<Option<crate::FieldValue>, XmtpError> {
                let group = self.inner.clone();
                let key = key.try_into()?;
                on_sdk_worker(self.inner.context.clone(), async move {
                    let value = group
                        .map_value(&field.into(), &key)
                        .map_err(XmtpError::from_group)?;
                    Ok(value.map(Into::into))
                })
                .await
            }

            /// User field values by inbox, read from one snapshot. Absent
            /// `fields` selects every user field, and absent `inbox_ids`
            /// every inbox with a value. A selected inbox with no values
            /// maps to an empty list.
            // implements: META-072
            pub async fn user_data(
                &self,
                fields: Option<Vec<crate::MetadataFieldRef>>,
                inbox_ids: Option<Vec<InboxId>>,
            ) -> Result<HashMap<InboxId, Vec<crate::UserFieldValue>>, XmtpError> {
                let group = self.inner.clone();
                let fields: Option<Vec<_>> =
                    fields.map(|fields| fields.into_iter().map(Into::into).collect());
                let inbox_ids = inbox_ids
                    .map(|ids| {
                        ids.iter()
                            .map(|id| crate::metadata::core_inbox_id(id.checked()?))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?;
                on_sdk_worker(self.inner.context.clone(), async move {
                    let data = group
                        .user_data(fields.as_deref(), inbox_ids.as_deref())
                        .map_err(XmtpError::from_group)?;
                    Ok(data
                        .into_iter()
                        .map(|(inbox_id, values)| {
                            (
                                InboxId::unchecked(inbox_id.to_hex()),
                                values.into_iter().map(Into::into).collect(),
                            )
                        })
                        .collect())
                })
                .await
            }

            /// Sets or clears the caller's own user field values in one
            /// commit. Nothing is committed when nothing changes.
            // implements: META-073
            pub async fn update_user_data(
                &self,
                values: Vec<crate::UserFieldUpdate>,
            ) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                let values: Vec<_> = values.into_iter().map(Into::into).collect();
                on_sdk_worker(
                    self.inner.context.clone(),
                    Box::pin(async move {
                        group
                            .update_user_data(&values)
                            .await
                            .map_err(XmtpError::from_group)
                    }),
                )
                .await
            }

            /// Applies `operation` to `field` in one commit, subject to the
            /// field's committed type and policies.
            // implements: META-071
            pub async fn update_metadata_field(
                &self,
                field: crate::MetadataFieldRef,
                operation: crate::ComponentMutation,
            ) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                let field = field.into();
                let operation = operation.try_into()?;
                on_sdk_worker(
                    self.inner.context.clone(),
                    Box::pin(async move {
                        group
                            .update_metadata_field(&field, &operation)
                            .await
                            .map_err(XmtpError::from_group)
                    }),
                )
                .await
            }
        }
    };
}

metadata_conversation!(Group);
metadata_conversation!(Dm);
