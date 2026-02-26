use diesel::result::DatabaseErrorKind;
/// Extra trait implementations for xmtp_db types
use rand::{
    Rng, RngExt,
    distr::{Distribution, StandardUniform},
    prelude::IteratorRandom,
};
use xmtp_proto::types::Cursor;

use crate::{
    DuplicateItem, NotFound, StorageError, refresh_state::EntityKind,
    sql_key_store::SqlKeyStoreError,
};

// choose a random db error in StorageError
// only cover errors that can happen in db access
impl Distribution<StorageError> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> StorageError {
        match rng.random_range(0..=13) {
            0 => StorageError::DieselConnect(rand_diesel_conn_err(rng)),
            1 => StorageError::DieselResult(rand_diesel_result(rng)),
            2 => StorageError::NotFound(rand::random()),
            3 => StorageError::Duplicate(DuplicateItem::WelcomeId(Some(Cursor::default()))),
            4 => rand::random(),
            5 => StorageError::IntentionalRollback,
            6 => StorageError::DbSerialize,
            7 => StorageError::DbDeserialize,
            8 => StorageError::Builder(derive_builder::UninitializedFieldError::new("test field")),
            10 => rand::random(), // platform
            11 => StorageError::Prost(
                <xmtp_proto::mls_v1::GroupMessage as prost::Message>::decode([].as_slice())
                    .unwrap_err(),
            ),
            12 => StorageError::Connection(rand::random()),
            13 => StorageError::Conversion(xmtp_proto::ConversionError::Unspecified(
                "random test error",
            )),
            _ => unreachable!(),
        }
    }
}

impl Distribution<SqlKeyStoreError> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> SqlKeyStoreError {
        use SqlKeyStoreError::*;
        match rng.random_range(0..=5) {
            0 => UnsupportedValueTypeBytes,
            1 => UnsupportedMethod,
            2 => SerializationError,
            3 => NotFound,
            4 => Storage(rand_diesel_result(rng)),
            5 => Connection(rand::random()),
            _ => unreachable!(),
        }
    }
}

impl Distribution<NotFound> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> NotFound {
        match rng.random_range(0..=13) {
            0 => NotFound::GroupByWelcome(Cursor::default()),
            1 => NotFound::GroupById(Vec::new()),
            2 => NotFound::InstallationTimeForGroup(Vec::new()),
            3 => NotFound::InboxIdForAddress("random test inbox".into()),
            4 => NotFound::MessageById(Vec::new()),
            5 => NotFound::DmByInbox("random dm by inbox".into()),
            6 => NotFound::IntentForToPublish(i32::MAX),
            7 => NotFound::IntentForPublish(i32::MAX),
            8 => NotFound::IntentForCommitted(i32::MIN),
            9 => NotFound::IntentById(i32::MIN),
            10 => NotFound::RefreshStateByIdKindAndOriginator(
                Vec::new(),
                EntityKind::ApplicationMessage,
                0,
            ),
            11 => NotFound::CipherSalt("random salt for testing".into()),
            12 => NotFound::SyncGroup(xmtp_common::rand_array::<32>().into()),
            13 => NotFound::MlsGroup,
            _ => unreachable!(),
        }
    }
}

impl Distribution<crate::ConnectionError> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> crate::ConnectionError {
        match rng.random_range(0..=1) {
            0 => crate::ConnectionError::Database(rand_diesel_result(rng)),
            1 => crate::ConnectionError::Platform(rand::random()),
            _ => unreachable!(),
        }
    }
}

fn rand_diesel_conn_err<R: Rng + ?Sized>(rng: &mut R) -> diesel::ConnectionError {
    use diesel::ConnectionError::*;
    vec![
        diesel::ConnectionError::InvalidCString(
            std::ffi::CString::new(b"f\0oo".to_vec()).unwrap_err(),
        ),
        BadConnection("rand bad connection err".into()),
        InvalidConnectionUrl("bad conn url".into()),
        CouldntSetupConfiguration(rand_diesel_result(rng)),
    ]
    .into_iter()
    .choose(rng)
    .unwrap()
}

fn rand_diesel_result<R: Rng + ?Sized>(rng: &mut R) -> diesel::result::Error {
    use diesel::result::Error::*;
    vec![
        InvalidCString(std::ffi::CString::new(b"f\0oo".to_vec()).unwrap_err()),
        DatabaseError(
            rand_db_error(rng),
            Box::new("Random Db Test Error".to_string()),
        ),
        diesel::result::Error::NotFound,
        QueryBuilderError(Box::new(std::io::Error::other("Rand query builder error"))),
        DeserializationError(Box::new(std::io::Error::other(
            "rand deserialization error",
        ))),
        SerializationError(Box::new(std::io::Error::other("Rand serialization error"))),
        RollbackTransaction,
        AlreadyInTransaction,
        NotInTransaction,
    ]
    .into_iter()
    .choose(rng)
    .unwrap()
}

fn rand_db_error<R: Rng + ?Sized>(rng: &mut R) -> DatabaseErrorKind {
    use DatabaseErrorKind::*;
    vec![
        UniqueViolation,
        ForeignKeyViolation,
        UnableToSendCommand,
        SerializationFailure,
        ReadOnlyTransaction,
        RestrictViolation,
        NotNullViolation,
        CheckViolation,
        ExclusionViolation,
        ClosedConnection,
        Unknown,
    ]
    .into_iter()
    .choose(rng)
    .unwrap()
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod native {
    use crate::PlatformStorageError;

    use super::*;
    impl Distribution<PlatformStorageError> for StandardUniform {
        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> PlatformStorageError {
            match rng.random_range(0..=9) {
                0 => PlatformStorageError::DbConnection(rand_r2d2_err(rng)),
                1 => PlatformStorageError::PoolNeedsConnection,
                2 => PlatformStorageError::SqlCipherNotLoaded,
                3 => PlatformStorageError::SqlCipherKeyIncorrect,
                4 => PlatformStorageError::DieselResult(rand_diesel_result(rng)),
                5 => PlatformStorageError::NotFound(rand::random()),
                6 => PlatformStorageError::Io(std::io::Error::other("test io error")),
                7 => PlatformStorageError::FromHex(hex::FromHexError::OddLength),
                8 => PlatformStorageError::DieselConnect(rand_diesel_conn_err(rng)),
                9 => {
                    PlatformStorageError::Boxed(Box::new(std::io::Error::other("test boxed error")))
                }
                _ => unreachable!(),
            }
        }
    }

    fn rand_r2d2_err<R: Rng + ?Sized>(rng: &mut R) -> diesel::r2d2::Error {
        match rng.random_range(0..=1) {
            0 => diesel::r2d2::Error::ConnectionError(rand_diesel_conn_err(rng)),
            1 => diesel::r2d2::Error::QueryError(rand_diesel_result(rng)),
            _ => unreachable!(),
        }
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
mod wasm {
    use crate::PlatformStorageError;

    use super::*;
    impl Distribution<crate::PlatformStorageError> for StandardUniform {
        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> crate::PlatformStorageError {
            match rng.random_range(0..=2) {
                0 => PlatformStorageError::SAH(sqlite_wasm_vfs::sahpool::OpfsSAHError::Generic(
                    "rand test opfs err".to_string(),
                )),
                1 => PlatformStorageError::Connection(rand_diesel_conn_err(rng)),
                2 => PlatformStorageError::DieselResult(rand_diesel_result(rng)),
                _ => unreachable!(),
            }
        }
    }
}
