//! SQL codecs for the `#[repr(i32)]` enums that are stored as integer columns.
//!
//! Both storage tracks encode these enums the same way, and the discriminants
//! are effectively a wire format: they are already written into every deployed
//! SQLite database, so a value's meaning can never change and a retired
//! discriminant can never be reused.
//!
//! [`impl_sql_int_enum!`] emits the diesel and the sqlx codecs from a single
//! list. That matters more on the async track than it looks: the async track
//! has no `diesel::table!` schema, so nothing there checks a column mapping at
//! compile time. Two hand-written copies of the mapping could drift silently in
//! exactly the place where nothing would catch it.

/// Returned when a stored integer does not correspond to any known variant.
///
/// Reaching this means the database holds a value written by a newer version of
/// the code (or is corrupt); the row is rejected rather than coerced.
#[derive(Debug, thiserror::Error)]
#[error("Unrecognized {type_name} variant {value}")]
pub struct UnrecognizedVariant {
    pub type_name: &'static str,
    pub value: i32,
}

/// Implements the SQL codecs for a `#[repr(i32)]` fieldless enum stored as an
/// integer column — diesel/SQLite under `sync`, sqlx/Postgres under `async`.
///
/// The discriminant list restates what the enum declaration already says. That
/// duplication is deliberate: it puts the wire format in one greppable place,
/// and the generated `const` assertions make any drift from the enum's own
/// discriminants a compile error.
///
/// ```ignore
/// impl_sql_int_enum!(ConsentState {
///     Unknown = 0,
///     Allowed = 1,
///     Denied = 2,
/// });
/// ```
#[macro_export]
macro_rules! impl_sql_int_enum {
    ($ty:ident { $($variant:ident = $disc:literal),+ $(,)? }) => {
        $(
            const _: () = assert!(
                $ty::$variant as i32 == $disc,
                concat!(
                    "discriminant for ", stringify!($ty), "::", stringify!($variant),
                    " disagrees with its SQL codec -- the stored value would change meaning",
                ),
            );
        )+

        #[cfg(any(feature = "sync", feature = "async"))]
        impl $ty {
            /// The stored integer form.
            const fn as_sql_int(self) -> i32 {
                self as i32
            }

            fn from_sql_int(
                value: i32,
            ) -> ::core::result::Result<Self, $crate::encrypted_store::sql_int_enum::UnrecognizedVariant>
            {
                match value {
                    $($disc => Ok($ty::$variant),)+
                    value => Err($crate::encrypted_store::sql_int_enum::UnrecognizedVariant {
                        type_name: stringify!($ty),
                        value,
                    }),
                }
            }
        }

        #[cfg(feature = "sync")]
        impl ::diesel::serialize::ToSql<::diesel::sql_types::Integer, ::diesel::sqlite::Sqlite>
            for $ty
        where
            i32: ::diesel::serialize::ToSql<::diesel::sql_types::Integer, ::diesel::sqlite::Sqlite>,
        {
            fn to_sql<'b>(
                &'b self,
                out: &mut ::diesel::serialize::Output<'b, '_, ::diesel::sqlite::Sqlite>,
            ) -> ::diesel::serialize::Result {
                out.set_value(self.as_sql_int());
                Ok(::diesel::serialize::IsNull::No)
            }
        }

        #[cfg(feature = "sync")]
        impl ::diesel::deserialize::FromSql<::diesel::sql_types::Integer, ::diesel::sqlite::Sqlite>
            for $ty
        where
            i32: ::diesel::deserialize::FromSql<
                ::diesel::sql_types::Integer,
                ::diesel::sqlite::Sqlite,
            >,
        {
            fn from_sql(
                bytes: <::diesel::sqlite::Sqlite as ::diesel::backend::Backend>::RawValue<'_>,
            ) -> ::diesel::deserialize::Result<Self> {
                Ok(Self::from_sql_int(i32::from_sql(bytes)?)?)
            }
        }

        // Postgres `INTEGER` is `int4`, so the codec delegates to `i32` rather
        // than restating the type mapping.
        #[cfg(all(feature = "async", not(feature = "sync")))]
        impl ::sqlx::Type<::sqlx::Postgres> for $ty {
            fn type_info() -> ::sqlx::postgres::PgTypeInfo {
                <i32 as ::sqlx::Type<::sqlx::Postgres>>::type_info()
            }

            fn compatible(ty: &::sqlx::postgres::PgTypeInfo) -> bool {
                <i32 as ::sqlx::Type<::sqlx::Postgres>>::compatible(ty)
            }
        }

        #[cfg(all(feature = "async", not(feature = "sync")))]
        impl ::sqlx::Encode<'_, ::sqlx::Postgres> for $ty {
            fn encode_by_ref(
                &self,
                buf: &mut ::sqlx::postgres::PgArgumentBuffer,
            ) -> ::core::result::Result<::sqlx::encode::IsNull, ::sqlx::error::BoxDynError> {
                <i32 as ::sqlx::Encode<::sqlx::Postgres>>::encode_by_ref(&self.as_sql_int(), buf)
            }
        }

        #[cfg(all(feature = "async", not(feature = "sync")))]
        impl<'r> ::sqlx::Decode<'r, ::sqlx::Postgres> for $ty {
            fn decode(
                value: ::sqlx::postgres::PgValueRef<'r>,
            ) -> ::core::result::Result<Self, ::sqlx::error::BoxDynError> {
                Ok(Self::from_sql_int(
                    <i32 as ::sqlx::Decode<'r, ::sqlx::Postgres>>::decode(value)?,
                )?)
            }
        }
    };
}
