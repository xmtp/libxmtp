use crate::encrypted_store::schema::identity;
use crate::schema::identity::dsl;
use crate::{ConnectionExt, DbConnection, StorageError, impl_fetch, impl_store};
use derive_builder::Builder;
use diesel::prelude::*;
use xmtp_common::NS_IN_SEC;
use xmtp_common::time::now_ns;

/// Identity of this installation
/// There can only be one.
#[derive(Insertable, Queryable, Debug, Clone, Builder)]
#[diesel(table_name = identity)]
#[builder(setter(into), build_fn(error = "crate::StorageError"))]
pub struct StoredIdentity {
    pub inbox_id: String,
    pub installation_keys: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    #[builder(setter(skip))]
    rowid: Option<i32>,
    pub next_key_package_rotation_ns: Option<i64>,
}

impl_fetch!(StoredIdentity, identity);
impl_store!(StoredIdentity, identity);

impl StoredIdentity {
    pub fn builder() -> StoredIdentityBuilder {
        StoredIdentityBuilder::default()
    }

    pub fn new(inbox_id: String, installation_keys: Vec<u8>, credential_bytes: Vec<u8>) -> Self {
        Self {
            inbox_id,
            installation_keys,
            credential_bytes,
            rowid: None,
            next_key_package_rotation_ns: None,
        }
    }
}
impl<C: ConnectionExt> DbConnection<C> {
    pub fn queue_key_package_rotation(&self) -> Result<(), StorageError> {
        let rotate_at_ns = now_ns() + 5 * NS_IN_SEC;

        self.raw_query_write(|conn| {
            // Fetch the identity row (assuming a single row exists)
            let identity = dsl::identity.first::<StoredIdentity>(conn)?;
            if identity.next_key_package_rotation_ns.is_none() {
                diesel::update(dsl::identity)
                    .set(dsl::next_key_package_rotation_ns.eq(rotate_at_ns))
                    .execute(conn)?;
            }

            Ok(())
        })?;

        Ok(())
    }

    pub fn reset_key_package_rotation_queue(
        &self,
        new_kp_valid_not_after: i64,
    ) -> Result<(), StorageError> {
        use crate::schema::identity::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::identity)
                .set(dsl::next_key_package_rotation_ns.eq(Some(new_kp_valid_not_after * NS_IN_SEC)))
                .execute(conn)?;
            Ok(())
        })?;

        Ok(())
    }

    pub fn is_identity_needs_rotation(&self) -> Result<bool, StorageError> {
        use crate::schema::identity::dsl;

        let next_rotation_opt: Option<i64> = self.raw_query_read(|conn| {
            dsl::identity
                .select(dsl::next_key_package_rotation_ns)
                .first::<Option<i64>>(conn)
        })?;

        Ok(match next_rotation_opt {
            Some(rotate_at) => now_ns() >= rotate_at,
            None => true,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::StoredIdentity;
    use crate::{Store, XmtpTestDb};
    use xmtp_common::rand_vec;

    #[xmtp_common::test]
    async fn can_only_store_one_identity() {
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = &store.conn();

        StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(conn)
            .unwrap();

        let duplicate_insertion =
            StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>()).store(conn);
        assert!(duplicate_insertion.is_err());
    }
}
