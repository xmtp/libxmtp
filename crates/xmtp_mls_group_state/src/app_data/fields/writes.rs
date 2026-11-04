//! Field writes: resolving a requested write against the committed type and current
//! value into app-data update operations.

use super::*;

impl<'a> FieldSnapshot<'a> {
    /// Encode `mutation` of `field` under the field's committed type.
    // implements: META-071
    pub fn field_write(
        &self,
        field: &MetadataFieldRef,
        mutation: &ComponentMutation,
    ) -> Result<FieldWrite, FieldError> {
        use MetadataComponentType as T;
        use MetadataKeyType as K;
        let descriptor = self.resolve(field)?;
        let id = descriptor.field.component_id;
        let ty = descriptor.component_type;
        let component_type = wire_type(id, ty)?;
        let payload = match (ty, mutation) {
            (_, ComponentMutation::Remove) => {
                return Ok(FieldWrite {
                    component_id: id,
                    component_type,
                    operation: WriteOperation::Remove,
                });
            }
            (T::Bytes, ComponentMutation::Replace(value)) => {
                scalar(MetadataScalarType::Bytes, value).map(|b| Ok(b.to_vec()))
            }
            (T::String, ComponentMutation::Replace(value)) => {
                scalar(MetadataScalarType::String, value).map(|b| Ok(b.to_vec()))
            }
            (
                T::Map {
                    key_type: K::Bytes,
                    value_type,
                },
                ComponentMutation::MapDelta(mutations),
            ) => map_delta(mutations, value_type, bytes_key).map(|d| d.tls_serialize_detached()),
            (
                T::Map {
                    key_type: K::InboxId,
                    value_type,
                },
                ComponentMutation::MapDelta(mutations),
            ) => map_delta(mutations, value_type, inbox_key).map(|d| d.tls_serialize_detached()),
            (T::Set { key_type: K::Bytes }, ComponentMutation::SetDelta(mutations)) => {
                set_delta(mutations, bytes_key).map(|d| d.tls_serialize_detached())
            }
            (
                T::Set {
                    key_type: K::InboxId,
                },
                ComponentMutation::SetDelta(mutations),
            ) => set_delta(mutations, inbox_key).map(|d| d.tls_serialize_detached()),
            _ => None,
        }
        .ok_or(FieldError::TypeMismatch(id))??;
        Ok(FieldWrite {
            component_id: id,
            component_type,
            operation: WriteOperation::Update(payload),
        })
    }

    /// Encode the writer's own-key sets and clears of user fields.
    // implements: META-073
    pub fn user_data_writes(
        &self,
        values: &[UserFieldUpdate],
    ) -> Result<Vec<FieldWrite>, FieldError> {
        if let Some(id) = first_duplicate(values.iter().map(|v| v.field.component_id)) {
            return Err(FieldError::DuplicateField(id));
        }
        values
            .iter()
            .map(|UserFieldUpdate { field, value }| {
                let descriptor = self.resolve(field)?;
                let id = descriptor.field.component_id;
                let value_type = descriptor
                    .user_value_type()
                    .ok_or(FieldError::NotUserField(id))?;
                let operation = match value {
                    Some(value) => WriteOperation::SetOwn(
                        scalar(value_type, value)
                            .ok_or(FieldError::TypeMismatch(id))?
                            .to_vec(),
                    ),
                    None => WriteOperation::ClearOwn,
                };
                Ok(FieldWrite {
                    component_id: id,
                    component_type: wire_type(id, descriptor.component_type)?,
                    operation,
                })
            })
            .collect()
    }

    /// The `AppDataUpdate` operations that carry out `writes` for the
    /// inbox `own`, given the current component values in `values`: this
    /// snapshot's dictionary with any pending updates applied.
    ///
    /// Types and the field list come from this snapshot, which must be the
    /// committed one. Each write is refused if its field is gone
    /// or changed type. Every payload is applied to its current value, so
    /// bounds, UTF-8, and key presence fail here rather than in the commit.
    /// A write that changes nothing is dropped: a clear of an absent key, a
    /// removal of an absent component, or an update whose result equals the
    /// current bytes.
    // implements: META-071, META-073
    pub fn resolve_writes(
        &self,
        values: Option<&AppDataDictionary>,
        own: InboxId,
        writes: &[FieldWrite],
    ) -> Result<Vec<(ComponentId, AppDataUpdateOperation)>, FieldError> {
        self.operations(values, own, writes, false)
    }

    /// The operations of `writes` as [`Self::resolve_writes`] gives them,
    /// keeping an update whose result equals the current bytes: the
    /// operations a receiver would check if they were published.
    // implements: META-071
    pub fn write_operations(
        &self,
        values: Option<&AppDataDictionary>,
        own: InboxId,
        writes: &[FieldWrite],
    ) -> Result<Vec<(ComponentId, AppDataUpdateOperation)>, FieldError> {
        self.operations(values, own, writes, true)
    }

    fn operations(
        &self,
        values: Option<&AppDataDictionary>,
        own: InboxId,
        writes: &[FieldWrite],
        keep_unchanged: bool,
    ) -> Result<Vec<(ComponentId, AppDataUpdateOperation)>, FieldError> {
        if let Some(id) = first_duplicate(writes.iter().map(|w| w.component_id)) {
            return Err(FieldError::DuplicateField(id));
        }
        writes
            .iter()
            .filter_map(|write| {
                self.resolve_write(values, own, write, keep_unchanged)
                    .map(|op| op.map(|op| (write.component_id, op)))
                    .transpose()
            })
            .collect()
    }

    fn resolve_write(
        &self,
        values: Option<&AppDataDictionary>,
        own: InboxId,
        write: &FieldWrite,
        keep_unchanged: bool,
    ) -> Result<Option<AppDataUpdateOperation>, FieldError> {
        let id = write.component_id;
        let descriptor = self.resolve_id(id)?;
        let actual = descriptor.component_type;
        if actual.tag() != Some(write.component_type) {
            return Err(FieldError::TypeChanged {
                component_id: id,
                expected: write.component_type,
                actual,
            });
        }
        let own_key = matches!(
            write.operation,
            WriteOperation::SetOwn(_) | WriteOperation::ClearOwn
        );
        if own_key && !descriptor.is_user_field() {
            return Err(FieldError::TypeMismatch(id));
        }
        let current = values.and_then(|d| d.get(&id.as_u16()));
        let owned = || -> Result<bool, FieldError> {
            Ok(current
                .map(TlsMap::<InboxId, VLBytes>::tls_deserialize_exact)
                .transpose()
                .map_err(|e| malformed(id, e))?
                .is_some_and(|map| map.contains_key(&own)))
        };
        let delta = TlsMapDelta::<InboxId, VLBytes>::new();
        let payload = match &write.operation {
            WriteOperation::Remove => return Ok(current.map(|_| AppDataUpdateOperation::Remove)),
            WriteOperation::Update(payload) => payload.clone(),
            WriteOperation::SetOwn(value) if owned()? => delta
                .update(own, value.as_slice().into())
                .tls_serialize_detached()?,
            WriteOperation::SetOwn(value) => delta
                .insert(own, value.as_slice().into())
                .tls_serialize_detached()?,
            WriteOperation::ClearOwn if owned()? => delta.delete(own).tls_serialize_detached()?,
            WriteOperation::ClearOwn => return Ok(None),
        };
        let next = apply_app_data_update_payload(id, &payload, current, &self.registry)?;
        Ok((keep_unchanged || current != Some(next.as_slice()))
            .then(|| AppDataUpdateOperation::Update(payload.into())))
    }
}

/// The bytes of `value` if it has type `ty`.
fn scalar(ty: MetadataScalarType, value: &FieldValue) -> Option<&[u8]> {
    match (ty, value) {
        (MetadataScalarType::Bytes, FieldValue::Bytes(bytes)) => Some(bytes),
        (MetadataScalarType::String, FieldValue::String(s)) => Some(s.as_bytes()),
        _ => None,
    }
}
fn bytes_key(key: &FieldKey) -> Option<VLBytes> {
    match key {
        FieldKey::Bytes(bytes) => Some(bytes.as_slice().into()),
        FieldKey::InboxId(_) => None,
    }
}
fn inbox_key(key: &FieldKey) -> Option<InboxId> {
    match key {
        FieldKey::InboxId(inbox) => Some(*inbox),
        FieldKey::Bytes(_) => None,
    }
}

/// The delta of `mutations`, or `None` if a key or value has the wrong type.
fn map_delta<K>(
    mutations: &[MapMutation],
    value_type: MetadataScalarType,
    key: fn(&FieldKey) -> Option<K>,
) -> Option<TlsMapDelta<K, VLBytes>> {
    mutations
        .iter()
        .try_fold(TlsMapDelta::new(), |delta, mutation| {
            Some(match mutation {
                MapMutation::Insert(k, v) => delta.insert(key(k)?, scalar(value_type, v)?.into()),
                MapMutation::Update(k, v) => delta.update(key(k)?, scalar(value_type, v)?.into()),
                MapMutation::Delete(k) => delta.delete(key(k)?),
            })
        })
}

/// The delta of `mutations`, or `None` if a key has the wrong type.
fn set_delta<K: Serialize + Size>(
    mutations: &[SetMutation],
    key: fn(&FieldKey) -> Option<K>,
) -> Option<TlsSetDelta<K>> {
    mutations
        .iter()
        .try_fold(TlsSetDelta::new(), |delta, mutation| {
            Some(match mutation {
                SetMutation::Insert(k) => delta.insert(key(k)?),
                SetMutation::Delete(k) => delta.remove(key(k)?),
                SetMutation::DeleteByHash(hash) => {
                    delta.remove_by_hash(TlsKeyHash::from_bytes(*hash))
                }
            })
        })
}
pub(super) fn first_duplicate(ids: impl IntoIterator<Item = ComponentId>) -> Option<ComponentId> {
    let mut seen = BTreeSet::new();
    ids.into_iter().find(|id| !seen.insert(*id))
}
