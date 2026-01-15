//! Check that certain pragmas are set

use crate::{ConnectionExt, DbConnection};
use diesel::prelude::*;

#[derive(QueryableByName, Debug)]
struct BusyTimeout {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    timeout: i32,
}

pub trait Pragmas {
    /// Check the busy timeout value
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError>;
    fn set_sqlcipher_log<S: AsRef<str>>(&self, level: S) -> Result<(), crate::ConnectionError>;
}

impl<T> Pragmas for &T
where
    T: Pragmas,
{
    /// Check the busy timeout value
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError> {
        (**self).busy_timeout()
    }

    fn set_sqlcipher_log<S: AsRef<str>>(&self, level: S) -> Result<(), crate::ConnectionError> {
        (**self).set_sqlcipher_log(level)
    }
}

impl<C: ConnectionExt> Pragmas for DbConnection<C> {
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            let BusyTimeout { timeout } =
                diesel::sql_query("PRAGMA busy_timeout").get_result::<BusyTimeout>(conn)?;
            Ok(timeout)
        })
    }

    fn set_sqlcipher_log<S: AsRef<str>>(&self, level: S) -> Result<(), crate::ConnectionError> {
        let level = level.as_ref();
        self.raw_query_read(|conn| {
            diesel::sql_query(format!("PRAGMA cipher_log_level = {level}")).execute(conn)?;
            Ok(())
        })
    }
}
