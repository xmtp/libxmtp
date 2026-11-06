//! App-data update intents.

use super::*;

/// Payload of [`crate::groups::intents::queue::QueueIntent::app_data_update`].
///
/// Serialized as the local-only `xmtp.mls.database.AppDataUpdateData`
/// proto (`proto/mls/database/intents.proto`); it never crosses the MLS
/// wire. Component IDs are `u32` in the proto and narrowed back here;
/// a value above `u16::MAX` or an unknown version variant fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppDataUpdateIntentData {
    /// One verbatim `AppDataUpdate` payload (V1). Its meaning follows the
    /// target component's registered `ComponentType`: a scalar's new
    /// value, or a TLS-encoded map or set delta.
    Payload { component_id: u16, payload: Vec<u8> },
    /// Typed metadata field writes, committed together (V2). The
    /// publisher resolves them with
    /// [`xmtp_mls_group_state::app_data::fields::FieldSnapshot::resolve_writes`]
    /// against the committed registry.
    Fields(Vec<FieldWrite>),
}

impl AppDataUpdateIntentData {
    /// Write `payload` to `component_id`.
    pub fn new(component_id: u16, payload: Vec<u8>) -> Self {
        Self::Payload {
            component_id,
            payload,
        }
    }
}

impl From<AppDataUpdateIntentData> for Vec<u8> {
    fn from(intent: AppDataUpdateIntentData) -> Self {
        use xmtp_proto::xmtp::mls::database::{
            AppDataUpdateData,
            app_data_update_data::{
                FieldWrite as FieldWriteProto, V1, V2, Version, field_write::Operation,
            },
        };
        let version = match intent {
            AppDataUpdateIntentData::Payload {
                component_id,
                payload,
            } => Version::V1(V1 {
                component_id: component_id.into(),
                payload,
            }),
            AppDataUpdateIntentData::Fields(writes) => Version::V2(V2 {
                writes: writes
                    .into_iter()
                    .map(|write| FieldWriteProto {
                        component_id: write.component_id.as_u16().into(),
                        component_type: write.component_type.into(),
                        operation: Some(match write.operation {
                            WriteOperation::Update(payload) => Operation::Update(payload),
                            WriteOperation::Remove => Operation::Remove(true),
                            WriteOperation::SetOwn(value) => Operation::SetOwn(value),
                            WriteOperation::ClearOwn => Operation::ClearOwn(true),
                        }),
                    })
                    .collect(),
            }),
        };
        AppDataUpdateData {
            version: Some(version),
        }
        .encode_to_vec()
    }
}

impl TryFrom<Vec<u8>> for AppDataUpdateIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        Self::try_from(data.as_slice())
    }
}

impl TryFrom<&[u8]> for AppDataUpdateIntentData {
    type Error = IntentError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        use xmtp_proto::xmtp::mls::database::{
            AppDataUpdateData,
            app_data_update_data::{Version, field_write::Operation},
        };
        let invalid =
            |reason: String| IntentError::Generic(format!("AppDataUpdateIntentData {reason}"));
        let component_id = |id: u32| {
            u16::try_from(id).map_err(|_| invalid(format!("component_id {id} exceeds u16 range")))
        };
        match AppDataUpdateData::decode(data)?.version {
            Some(Version::V1(v1)) => Ok(Self::Payload {
                component_id: component_id(v1.component_id)?,
                payload: v1.payload,
            }),
            Some(Version::V2(v2)) => v2
                .writes
                .into_iter()
                .map(|write| {
                    Ok(FieldWrite {
                        component_id: ComponentId::new(component_id(write.component_id)?),
                        component_type: ComponentType::try_from(write.component_type).map_err(
                            |_| {
                                invalid(format!(
                                    "component_type {} is unknown",
                                    write.component_type
                                ))
                            },
                        )?,
                        operation: match write.operation {
                            Some(Operation::Update(payload)) => WriteOperation::Update(payload),
                            Some(Operation::Remove(_)) => WriteOperation::Remove,
                            Some(Operation::SetOwn(value)) => WriteOperation::SetOwn(value),
                            Some(Operation::ClearOwn(_)) => WriteOperation::ClearOwn,
                            None => return Err(invalid("field write has no operation".into())),
                        },
                    })
                })
                .collect::<Result<_, _>>()
                .map(Self::Fields),
            None => Err(invalid("missing version oneof variant".into())),
        }
    }
}
