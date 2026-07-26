#![forbid(unsafe_code)]
//! # sqlx Postgres Storage Provider
//!
//! This crate implements an OpenMLS storage provider backed by sqlx and
//! PostgreSQL. It is the async-track counterpart of the sqlx-SQLite provider:
//! the OpenMLS storage trait is async here (the `maybe_async` openmls fork with
//! its `sync` feature off), so every method is a native `async fn`.
//!
//! The main struct is [`PostgresStorageProvider`], which implements the
//! [`StorageProvider`](openmls_traits::storage::StorageProvider) trait from the
//! `openmls_traits` crate.
//!
//! The crate manages its own database migrations in its own migrations table
//! with the name `_openmls_sqlx_migrations`. All tables created by this crate
//! are prefixed with `openmls_` to avoid name clashes.

use std::marker::PhantomData;

use futures_util::lock::Mutex;
use openmls_traits::storage::{CURRENT_VERSION, Entity, Key};
use serde::Serialize;
use sqlx::{Connection, Executor, PgConnection};

pub use crate::codec::Codec;

mod codec;
mod group_data;
mod storage_provider;
mod wrappers;

/// [`PostgresStorageProvider`] implements the
/// [`StorageProvider`](openmls_traits::storage::StorageProvider) trait and can
/// thus be used as a storage provider for OpenMLS.
///
/// It is generic over any codec `C` that implements the [`Codec`] trait.
/// The codec is used to serialize and deserialize the data stored in the
/// underlying database.
///
/// Like the SQLite provider, it borrows a single connection so the `&self`
/// trait methods can obtain the `&mut PgConnection` sqlx needs. It uses an async
/// [`Mutex`] (rather than a `RefCell`) so the guard held across `.await` is
/// `Send` — the `StorageProvider` futures must be `Send` on this track. Holding
/// one connection (rather than a pool) is deliberate: a single OpenMLS operation
/// issues many storage calls the host is expected to commit together within one
/// transaction. The provider is used single-threaded, so the mutex never contends.
pub struct PostgresStorageProvider<'a, C> {
    connection: Mutex<&'a mut PgConnection>,
    // `fn() -> C` (not `C`) so the phantom marker imposes no `Send`/`Sync` bound on
    // `C`: the provider must be `Sync` for its `&self` futures to be `Send`, and a
    // bare `PhantomData<C>` would make that hinge on `C: Sync`.
    codec: PhantomData<fn() -> C>,
}

impl<'a, C: Codec> PostgresStorageProvider<'a, C> {
    /// Create a new [`PostgresStorageProvider`] based on the given
    /// [`PgConnection`].
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection: Mutex::new(connection),
            codec: PhantomData,
        }
    }

    /// Run the migrations for the storage provider against the borrowed
    /// connection. Delegates to the free [`run_migrations`] function.
    pub async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        let mut conn = self.connection.lock().await;
        run_migrations(&mut **conn).await
    }

    fn wrap_storable_group_id_ref<'b, GroupId: Key<CURRENT_VERSION>>(
        &self,
        group_id: &'b GroupId,
    ) -> StorableGroupIdRef<'b, GroupId, C> {
        StorableGroupIdRef(group_id, PhantomData)
    }
}

/// The embedded schema migrations, applied in order: `(version, description, sql)`.
///
/// Versions are the source SQLite migrations' timestamps, kept identical so the
/// two providers describe the same schema history.
static MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        20250929142827,
        "init",
        include_str!("../migrations/20250929142827_init.sql"),
    ),
    (
        20251105171410,
        "add_application_export_tree",
        include_str!("../migrations/20251105171410_add_application_export_tree.sql"),
    ),
];

/// Apply this crate's migrations to a Postgres database over the given
/// connection. Safe to call repeatedly: already-applied migrations are skipped.
///
/// This is the primary migration entry point; use it to bring a fresh database
/// up to schema before constructing a [`PostgresStorageProvider`]. Applied
/// migrations are tracked in a dedicated `_openmls_sqlx_migrations` table so
/// this crate's bookkeeping never clashes with another sqlx migrator sharing
/// the same database.
pub async fn run_migrations(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    (&mut *conn)
        .execute(
            "CREATE TABLE IF NOT EXISTS _openmls_sqlx_migrations (\
             version BIGINT PRIMARY KEY, \
             description TEXT NOT NULL, \
             installed_on TIMESTAMPTZ NOT NULL DEFAULT now())",
        )
        .await?;

    for &(version, description, sql) in MIGRATIONS {
        let applied: Option<(i64,)> =
            sqlx::query_as("SELECT version FROM _openmls_sqlx_migrations WHERE version = $1")
                .bind(version)
                .fetch_optional(&mut *conn)
                .await?;
        if applied.is_some() {
            continue;
        }

        // Postgres runs DDL transactionally, so the schema change and its
        // bookkeeping row commit together or not at all.
        let mut tx = conn.begin().await?;
        tx.execute(sql).await?;
        sqlx::query("INSERT INTO _openmls_sqlx_migrations (version, description) VALUES ($1, $2)")
            .bind(version)
            .bind(description)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }

    Ok(())
}

// Static proof that the provider satisfies the OpenMLS storage trait at the
// current storage version, for every codec. Type-checking `assertion`'s body
// forces the compiler to resolve the trait bound even though it is never
// called; a missing or mismatched method would be a hard error here.
const _: () = {
    #[allow(dead_code)]
    fn assert_storage_provider<P: openmls_traits::storage::StorageProvider<CURRENT_VERSION>>() {}
    #[allow(dead_code)]
    fn assertion<'a, C: Codec + Send + Sync>() {
        assert_storage_provider::<PostgresStorageProvider<'a, C>>();
    }
};

#[derive(Debug, Serialize)]
struct KeyRefWrapper<'a, T: Key<CURRENT_VERSION>, C: Codec>(&'a T, PhantomData<C>);

impl<'a, T: Key<CURRENT_VERSION>, C: Codec> KeyRefWrapper<'a, T, C> {
    fn new(value: &'a T) -> Self {
        Self(value, PhantomData)
    }
}

struct EntityRefWrapper<'a, T: Entity<CURRENT_VERSION>, C: Codec>(&'a T, PhantomData<C>);

impl<'a, T: Entity<CURRENT_VERSION>, C: Codec> EntityRefWrapper<'a, T, C> {
    fn new(value: &'a T) -> Self {
        Self(value, PhantomData)
    }
}

struct EntitySliceWrapper<'a, T: Entity<CURRENT_VERSION>, C: Codec>(&'a [T], PhantomData<C>);

struct StorableGroupIdRef<'a, GroupId: Key<CURRENT_VERSION>, C: Codec>(&'a GroupId, PhantomData<C>);
