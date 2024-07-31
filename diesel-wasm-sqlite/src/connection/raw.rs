#![allow(unsafe_code)] // ffi calls

// use std::ffi::{CString, NulError};
// use std::io::{stderr, Write};
// use std::os::raw as libc;
// use std::ptr::NonNull;
// use std::{mem, ptr, slice, str};

// use super::functions::{build_sql_function_args, process_sql_function_result};
// use super::serialized_database::SerializedDatabase;
// use super::stmt::ensure_sqlite_ok;
// use super::{Sqlite, SqliteAggregateFunction};
// use crate::deserialize::FromSqlRow;
// use crate::result::Error::DatabaseError;
use crate::{
    sqlite_types::{SqliteFlags, SqliteOpenFlags},
    WasmSqlite, WasmSqliteError,
};
use diesel::result::*;
use diesel::serialize::ToSql;
use diesel::sql_types::HasSqlType;
use wasm_bindgen::{closure::Closure, JsValue};
/*
/// For use in FFI function, which cannot unwind.
/// Print the message, ask to open an issue at Github and [`abort`](std::process::abort).
macro_rules! assert_fail {
    ($fmt:expr $(,$args:tt)*) => {
        eprint!(concat!(
            $fmt,
            "If you see this message, please open an issue at https://github.com/diesel-rs/diesel/issues/new.\n",
            "Source location: {}:{}\n",
        ), $($args,)* file!(), line!());
        std::process::abort()
    };
}
*/

#[allow(missing_copy_implementations)]
#[derive(Clone, Debug)]
pub(super) struct RawConnection {
    pub(super) internal_connection: JsValue,
}

impl RawConnection {
    pub(super) async fn establish(database_url: &str) -> ConnectionResult<Self> {
        let sqlite3 = crate::get_sqlite().await;
        let database_url = if database_url.starts_with("sqlite://") {
            database_url.replacen("sqlite://", "file:", 1)
        } else {
            database_url.to_string()
        };
        let flags = SqliteOpenFlags::SQLITE_OPEN_READWRITE
            | SqliteOpenFlags::SQLITE_OPEN_CREATE
            | SqliteOpenFlags::SQLITE_OPEN_URI;

        Ok(RawConnection {
            internal_connection: sqlite3
                .open_v2(&database_url, Some(flags.bits() as i32))
                .await
                .map_err(WasmSqliteError::from)
                .map_err(ConnectionError::from)?,
        })
    }

    pub(super) async fn exec(&self, query: &str) -> QueryResult<()> {
        let sqlite3 = crate::get_sqlite().await;
        let result = sqlite3
            .exec(&self.internal_connection, query)
            .await
            .unwrap();

        Ok(result)
    }

    pub(super) fn rows_affected_by_last_query(&self) -> usize {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.changes(&self.internal_connection)
    }

    pub(super) fn register_sql_function<F>(
        &self,
        fn_name: &str,
        num_args: i32,
        deterministic: bool,
        mut f: F,
    ) -> QueryResult<()>
    where
        F: FnMut(JsValue, JsValue) + 'static,
    {
        let sqlite3 = crate::get_sqlite_unchecked();
        let flags = Self::get_flags(deterministic);

        let cb = Closure::new(f);
        sqlite3
            .create_function(
                &self.internal_connection,
                fn_name,
                num_args,
                flags,
                Some(&cb),
                None,
                None,
            )
            .unwrap();
        Ok(())
    }

    /*
    pub(super) fn register_aggregate_function<ArgsSqlType, RetSqlType, Args, Ret, A>(
        &self,
        fn_name: &str,
        num_args: usize,
    ) -> QueryResult<()>
    where
        A: SqliteAggregateFunction<Args, Output = Ret> + 'static + Send + std::panic::UnwindSafe,
        Args: FromSqlRow<ArgsSqlType, Sqlite>,
        Ret: ToSql<RetSqlType, Sqlite>,
        Sqlite: HasSqlType<RetSqlType>,
    {
        let fn_name = Self::get_fn_name(fn_name)?;
        let flags = Self::get_flags(false);

        let result = unsafe {
            ffi::sqlite3_create_function_v2(
                self.internal_connection.as_ptr(),
                fn_name.as_ptr(),
                num_args as _,
                flags,
                ptr::null_mut(),
                None,
                Some(run_aggregator_step_function::<_, _, _, _, A>),
                Some(run_aggregator_final_function::<_, _, _, _, A>),
                None,
            )
        };

        Self::process_sql_function_result(result)
    }

    pub(super) fn register_collation_function<F>(
        &self,
        collation_name: &str,
        collation: F,
    ) -> QueryResult<()>
    where
        F: Fn(&str, &str) -> std::cmp::Ordering + std::panic::UnwindSafe + Send + 'static,
    {
        let callback_fn = Box::into_raw(Box::new(CollationUserPtr {
            callback: collation,
            collation_name: collation_name.to_owned(),
        }));
        let collation_name = Self::get_fn_name(collation_name)?;

        let result = unsafe {
            ffi::sqlite3_create_collation_v2(
                self.internal_connection.as_ptr(),
                collation_name.as_ptr(),
                ffi::SQLITE_UTF8,
                callback_fn as *mut _,
                Some(run_collation_function::<F>),
                Some(destroy_boxed::<CollationUserPtr<F>>),
            )
        };

        let result = Self::process_sql_function_result(result);
        if result.is_err() {
            destroy_boxed::<CollationUserPtr<F>>(callback_fn as *mut _);
        }
        result
    }

    pub(super) fn serialize(&mut self) -> SerializedDatabase {
        unsafe {
            let mut size: ffi::sqlite3_int64 = 0;
            let data_ptr = ffi::sqlite3_serialize(
                self.internal_connection.as_ptr(),
                std::ptr::null(),
                &mut size as *mut _,
                0,
            );
            SerializedDatabase::new(data_ptr, size as usize)
        }
    }

    pub(super) fn deserialize(&mut self, data: &[u8]) -> QueryResult<()> {
        // the cast for `ffi::SQLITE_DESERIALIZE_READONLY` is required for old libsqlite3-sys versions
        #[allow(clippy::unnecessary_cast)]
        unsafe {
            let result = ffi::sqlite3_deserialize(
                self.internal_connection.as_ptr(),
                std::ptr::null(),
                data.as_ptr() as *mut u8,
                data.len() as i64,
                data.len() as i64,
                ffi::SQLITE_DESERIALIZE_READONLY as u32,
            );

            ensure_sqlite_ok(result, self.internal_connection.as_ptr())
        }
    }
    */

    fn get_flags(deterministic: bool) -> i32 {
        let mut flags = SqliteFlags::SQLITE_UTF8;
        if deterministic {
            flags |= SqliteFlags::SQLITE_DETERMINISTIC;
        }
        flags.bits() as i32
    }

    /*
    fn process_sql_function_result(result: i32) -> Result<(), Error> {
        if result == ffi::SQLITE_OK {
            Ok(())
        } else {
            let error_message = super::error_message(result);
            Err(DatabaseError(
                DatabaseErrorKind::Unknown,
                Box::new(error_message.to_string()),
            ))
        }
    }
    */
}
/*
impl Drop for RawConnection {
    fn drop(&mut self) {
        use std::thread::panicking;

        let sqlite3 = crate::get_sqlite_unchecked();

        let close_result = sqlite3.close(self.internal_connection);

        if close_result != ffi::SQLITE_OK {
            let error_message = super::error_message(close_result);
            if panicking() {
                write!(stderr(), "Error closing SQLite connection: {error_message}")
                    .expect("Error writing to `stderr`");
            } else {
                panic!("Error closing SQLite connection: {}", error_message);
            }
        }
    }
}
*/

/*
enum SqliteCallbackError {
    Abort(&'static str),
    DieselError(crate::result::Error),
    Panic(String),
}

impl SqliteCallbackError {
    fn emit(&self, ctx: *mut ffi::sqlite3_context) {
        let s;
        let msg = match self {
            SqliteCallbackError::Abort(msg) => *msg,
            SqliteCallbackError::DieselError(e) => {
                s = e.to_string();
                &s
            }
            SqliteCallbackError::Panic(msg) => msg,
        };
        unsafe {
            context_error_str(ctx, msg);
        }
    }
}

impl From<crate::result::Error> for SqliteCallbackError {
    fn from(e: crate::result::Error) -> Self {
        Self::DieselError(e)
    }
}

struct CustomFunctionUserPtr<F> {
    callback: F,
    function_name: String,
}
*/

/*
// Need a custom option type here, because the std lib one does not have guarantees about the discriminate values
// See: https://github.com/rust-lang/rfcs/blob/master/text/2195-really-tagged-unions.md#opaque-tags
#[repr(u8)]
enum OptionalAggregator<A> {
    // Discriminant is 0
    None,
    Some(A),
}

#[allow(warnings)]
extern "C" fn run_aggregator_step_function<ArgsSqlType, RetSqlType, Args, Ret, A>(
    ctx: *mut ffi::sqlite3_context,
    num_args: libc::c_int,
    value_ptr: *mut *mut ffi::sqlite3_value,
) where
    A: SqliteAggregateFunction<Args, Output = Ret> + 'static + Send + std::panic::UnwindSafe,
    Args: FromSqlRow<ArgsSqlType, Sqlite>,
    Ret: ToSql<RetSqlType, Sqlite>,
    Sqlite: HasSqlType<RetSqlType>,
{
    let result = std::panic::catch_unwind(move || {
        let args = unsafe { slice::from_raw_parts_mut(value_ptr, num_args as _) };
        run_aggregator_step::<A, Args, ArgsSqlType>(ctx, args)
    })
    .unwrap_or_else(|e| {
        Err(SqliteCallbackError::Panic(format!(
            "{}::step() panicked",
            std::any::type_name::<A>()
        )))
    });

    match result {
        Ok(()) => {}
        Err(e) => e.emit(ctx),
    }
}

fn run_aggregator_step<A, Args, ArgsSqlType>(
    ctx: *mut ffi::sqlite3_context,
    args: &mut [*mut ffi::sqlite3_value],
) -> Result<(), SqliteCallbackError>
where
    A: SqliteAggregateFunction<Args>,
    Args: FromSqlRow<ArgsSqlType, Sqlite>,
{
    static NULL_AG_CTX_ERR: &str = "An unknown error occurred. sqlite3_aggregate_context returned a null pointer. This should never happen.";
    static NULL_CTX_ERR: &str =
        "We've written the aggregator to the aggregate context, but it could not be retrieved.";

    let aggregate_context = unsafe {
        // This block of unsafe code makes the following assumptions:
        //
        // * sqlite3_aggregate_context allocates sizeof::<OptionalAggregator<A>>
        //   bytes of zeroed memory as documented here:
        //   https://www.sqlite.org/c3ref/aggregate_context.html
        //   A null pointer is returned for negative or zero sized types,
        //   which should be impossible in theory. We check that nevertheless
        //
        // * OptionalAggregator::None has a discriminant of 0 as specified by
        //   #[repr(u8)] + RFC 2195
        //
        // * If all bytes are zero, the discriminant is also zero, so we can
        //   assume that we get OptionalAggregator::None in this case. This is
        //   not UB as we only access the discriminant here, so we do not try
        //   to read any other zeroed memory. After that we initialize our enum
        //   by writing a correct value at this location via ptr::write_unaligned
        //
        // * We use ptr::write_unaligned as we did not found any guarantees that
        //   the memory will have a correct alignment.
        //   (Note I(weiznich): would assume that it is aligned correctly, but we
        //    we cannot guarantee it, so better be safe than sorry)
        ffi::sqlite3_aggregate_context(ctx, std::mem::size_of::<OptionalAggregator<A>>() as i32)
    };
    let aggregate_context = NonNull::new(aggregate_context as *mut OptionalAggregator<A>);
    let aggregator = unsafe {
        match aggregate_context.map(|a| &mut *a.as_ptr()) {
            Some(&mut OptionalAggregator::Some(ref mut agg)) => agg,
            Some(a_ptr @ &mut OptionalAggregator::None) => {
                ptr::write_unaligned(a_ptr as *mut _, OptionalAggregator::Some(A::default()));
                if let OptionalAggregator::Some(ref mut agg) = a_ptr {
                    agg
                } else {
                    return Err(SqliteCallbackError::Abort(NULL_CTX_ERR));
                }
            }
            None => {
                return Err(SqliteCallbackError::Abort(NULL_AG_CTX_ERR));
            }
        }
    };
    let args = build_sql_function_args::<ArgsSqlType, Args>(args)?;

    aggregator.step(args);
    Ok(())
}

extern "C" fn run_aggregator_final_function<ArgsSqlType, RetSqlType, Args, Ret, A>(
    ctx: *mut ffi::sqlite3_context,
) where
    A: SqliteAggregateFunction<Args, Output = Ret> + 'static + Send,
    Args: FromSqlRow<ArgsSqlType, Sqlite>,
    Ret: ToSql<RetSqlType, Sqlite>,
    Sqlite: HasSqlType<RetSqlType>,
{
    static NO_AGGREGATOR_FOUND: &str = "We've written to the aggregator in the xStep callback. If xStep was never called, then ffi::sqlite_aggregate_context() would have returned a NULL pointer.";
    let aggregate_context = unsafe {
        // Within the xFinal callback, it is customary to set nBytes to 0 so no pointless memory
        // allocations occur, a null pointer is returned in this case
        // See: https://www.sqlite.org/c3ref/aggregate_context.html
        //
        // For the reasoning about the safety of the OptionalAggregator handling
        // see the comment in run_aggregator_step_function.
        ffi::sqlite3_aggregate_context(ctx, 0)
    };

    let result = std::panic::catch_unwind(|| {
        let mut aggregate_context = NonNull::new(aggregate_context as *mut OptionalAggregator<A>);

        let aggregator = if let Some(a) = aggregate_context.as_mut() {
            let a = unsafe { a.as_mut() };
            match std::mem::replace(a, OptionalAggregator::None) {
                OptionalAggregator::None => {
                    return Err(SqliteCallbackError::Abort(NO_AGGREGATOR_FOUND));
                }
                OptionalAggregator::Some(a) => Some(a),
            }
        } else {
            None
        };

        let res = A::finalize(aggregator);
        let value = process_sql_function_result(&res)?;
        // We've checked already that ctx is not null
        unsafe {
            value.result_of(&mut *ctx);
        }
        Ok(())
    })
    .unwrap_or_else(|_e| {
        Err(SqliteCallbackError::Panic(format!(
            "{}::finalize() panicked",
            std::any::type_name::<A>()
        )))
    });
    if let Err(e) = result {
        e.emit(ctx);
    }
}

unsafe fn context_error_str(ctx: *mut ffi::sqlite3_context, error: &str) {
    ffi::sqlite3_result_error(ctx, error.as_ptr() as *const _, error.len() as _);
}

struct CollationUserPtr<F> {
    callback: F,
    collation_name: String,
}

#[allow(warnings)]
extern "C" fn run_collation_function<F>(
    user_ptr: *mut libc::c_void,
    lhs_len: libc::c_int,
    lhs_ptr: *const libc::c_void,
    rhs_len: libc::c_int,
    rhs_ptr: *const libc::c_void,
) -> libc::c_int
where
    F: Fn(&str, &str) -> std::cmp::Ordering + Send + std::panic::UnwindSafe + 'static,
{
    let user_ptr = user_ptr as *const CollationUserPtr<F>;
    let user_ptr = std::panic::AssertUnwindSafe(unsafe { user_ptr.as_ref() });

    let result = std::panic::catch_unwind(|| {
        let user_ptr = user_ptr.ok_or_else(|| {
            SqliteCallbackError::Abort(
                "Got a null pointer as data pointer. This should never happen",
            )
        })?;
        for (ptr, len, side) in &[(rhs_ptr, rhs_len, "rhs"), (lhs_ptr, lhs_len, "lhs")] {
            if *len < 0 {
                assert_fail!(
                    "An unknown error occurred. {}_len is negative. This should never happen.",
                    side
                );
            }
            if ptr.is_null() {
                assert_fail!(
                "An unknown error occurred. {}_ptr is a null pointer. This should never happen.",
                side
            );
            }
        }

        let (rhs, lhs) = unsafe {
            // Depending on the eTextRep-parameter to sqlite3_create_collation_v2() the strings can
            // have various encodings. register_collation_function() always selects SQLITE_UTF8, so the
            // pointers point to valid UTF-8 strings (assuming correct behavior of libsqlite3).
            (
                str::from_utf8(slice::from_raw_parts(rhs_ptr as *const u8, rhs_len as _)),
                str::from_utf8(slice::from_raw_parts(lhs_ptr as *const u8, lhs_len as _)),
            )
        };

        let rhs =
            rhs.map_err(|_| SqliteCallbackError::Abort("Got an invalid UTF-8 string for rhs"))?;
        let lhs =
            lhs.map_err(|_| SqliteCallbackError::Abort("Got an invalid UTF-8 string for lhs"))?;

        Ok((user_ptr.callback)(rhs, lhs))
    })
    .unwrap_or_else(|p| {
        Err(SqliteCallbackError::Panic(
            user_ptr
                .map(|u| u.collation_name.clone())
                .unwrap_or_default(),
        ))
    });

    match result {
        Ok(std::cmp::Ordering::Less) => -1,
        Ok(std::cmp::Ordering::Equal) => 0,
        Ok(std::cmp::Ordering::Greater) => 1,
        Err(SqliteCallbackError::Abort(a)) => {
            eprintln!(
                "Collation function {} failed with: {}",
                user_ptr
                    .map(|c| &c.collation_name as &str)
                    .unwrap_or_default(),
                a
            );
            std::process::abort()
        }
        Err(SqliteCallbackError::DieselError(e)) => {
            eprintln!(
                "Collation function {} failed with: {}",
                user_ptr
                    .map(|c| &c.collation_name as &str)
                    .unwrap_or_default(),
                e
            );
            std::process::abort()
        }
        Err(SqliteCallbackError::Panic(msg)) => {
            eprintln!("Collation function {} panicked", msg);
            std::process::abort()
        }
    }
}

extern "C" fn destroy_boxed<F>(data: *mut libc::c_void) {
    let ptr = data as *mut F;
    unsafe { std::mem::drop(Box::from_raw(ptr)) };
}
*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{AsyncConnection, WasmSqliteConnection};
    use diesel::connection::Connection;
    use wasm_bindgen_test::*;
    use web_sys::console;
    wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[wasm_bindgen_test]
    async fn test_fn_registration() {
        let mut result = WasmSqliteConnection::establish("test").await;
        let mut conn = result.unwrap();
        console::log_1(&"CONNECTED".into());
        conn.raw
            .register_sql_function("test", 0, true, |ctx, values| {
                console::log_1(&"Inside Fn".into());
            });
    }
}
