//! Check that certain pragmas are set

#[cfg(feature = "sync")]
use crate::{ConnectionExt, DbConnection};
#[cfg(feature = "sync")]
use diesel::prelude::*;

#[derive(Debug)]
#[cfg_attr(feature = "sync", derive(QueryableByName))]
struct BusyTimeout {
    #[cfg_attr(feature = "sync", diesel(sql_type = diesel::sql_types::Integer))]
    timeout: i32,
}

pub trait Pragmas {
    /// Check the busy timeout value
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError>;
    fn set_sqlcipher_log(&self, level: &str) -> Result<(), crate::ConnectionError>;
}

impl<T> Pragmas for &T
where
    T: Pragmas + xmtp_common::MaybeSync,
{
    /// Check the busy timeout value
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError> {
        (**self).busy_timeout()
    }

    fn set_sqlcipher_log(&self, level: &str) -> Result<(), crate::ConnectionError> {
        (**self).set_sqlcipher_log(level)
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> Pragmas for DbConnection<C> {
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError> {
        self.raw_query(|conn| {
            let BusyTimeout { timeout } =
                diesel::sql_query("PRAGMA busy_timeout").get_result::<BusyTimeout>(conn)?;
            Ok(timeout)
        })
    }

    fn set_sqlcipher_log(&self, level: &str) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::sql_query(format!("PRAGMA cipher_log_level = {level}")).execute(conn)?;
            Ok(())
        })
    }
}
