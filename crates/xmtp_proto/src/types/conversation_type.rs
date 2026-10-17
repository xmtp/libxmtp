use crate::ConversionError;
use crate::xmtp::device_sync::group_backup::ConversationTypeSave;
use crate::xmtp::mls::message_contents::ConversationType as ConversationTypeProto;
use serde::{Deserialize, Serialize};

#[cfg(feature = "diesel")]
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "diesel", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "diesel", diesel(sql_type = Integer))]
pub enum ConversationType {
    Group = 1,
    Dm = 2,
    Sync = 3,
    Oneshot = 4,
}

impl ConversationType {
    pub fn virtual_types() -> Vec<ConversationType> {
        vec![ConversationType::Sync, ConversationType::Oneshot]
    }

    pub fn is_virtual(&self) -> bool {
        // Use match to force exhaustive pattern matching
        match self {
            ConversationType::Group => false,
            ConversationType::Dm => false,
            ConversationType::Sync => true,
            ConversationType::Oneshot => true,
        }
    }
}

#[cfg(feature = "diesel")]
impl ToSql<Integer, Sqlite> for ConversationType
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

#[cfg(feature = "diesel")]
impl FromSql<Integer, Sqlite> for ConversationType
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(ConversationType::Group),
            2 => Ok(ConversationType::Dm),
            3 => Ok(ConversationType::Sync),
            4 => Ok(ConversationType::Oneshot),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl std::fmt::Display for ConversationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ConversationType::*;
        match self {
            Group => write!(f, "group"),
            Dm => write!(f, "dm"),
            Sync => write!(f, "sync"),
            Oneshot => write!(f, "oneshot"),
        }
    }
}

impl TryFrom<ConversationTypeSave> for ConversationType {
    type Error = ConversionError;
    fn try_from(value: ConversationTypeSave) -> Result<Self, Self::Error> {
        let conversation_type = match value {
            ConversationTypeSave::Dm => Self::Dm,
            ConversationTypeSave::Group => Self::Group,
            ConversationTypeSave::Sync => Self::Sync,
            ConversationTypeSave::Unspecified => {
                return Err(ConversionError::Unspecified("conversation_type"));
            }
        };
        Ok(conversation_type)
    }
}

impl From<ConversationType> for ConversationTypeSave {
    fn from(value: ConversationType) -> Self {
        match value {
            ConversationType::Dm => Self::Dm,
            ConversationType::Group => Self::Group,
            ConversationType::Sync => Self::Sync,
            ConversationType::Oneshot => Self::Unspecified,
        }
    }
}

/**
 * XMTP supports the following types of conversation
 *
 * *Group*: A conversation with 1->N members and complex permissions and roles
 * *DM*: A conversation between 2 members with simplified permissions
 * *Sync*: A conversation between all the devices of a single member with simplified permissions
 */
impl From<ConversationType> for ConversationTypeProto {
    fn from(value: ConversationType) -> Self {
        match value {
            ConversationType::Group => Self::Group,
            ConversationType::Dm => Self::Dm,
            ConversationType::Sync => Self::Sync,
            ConversationType::Oneshot => Self::Oneshot,
        }
    }
}

impl TryFrom<i32> for ConversationType {
    type Error = crate::ConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Group,
            2 => Self::Dm,
            3 => Self::Sync,
            4 => Self::Oneshot,
            n => {
                return Err(ConversionError::InvalidValue {
                    item: "ConversationType",
                    expected: "number between 1 - 4",
                    got: n.to_string(),
                });
            }
        })
    }
}
