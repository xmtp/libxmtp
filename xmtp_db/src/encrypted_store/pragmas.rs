//! Check that certain pragmas are set

use crate::{ConnectionExt, DbConnection};
use diesel::prelude::*;

#[derive(QueryableByName, Debug)]
struct BusyTimeout {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    timeout: i32,
}

pub trait CheckPragmas {
    /// Check the busy timeout value
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError>;
}

impl<T> CheckPragmas for &T
where
    T: CheckPragmas,
{
    /// Check the busy timeout value
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError> {
        (**self).busy_timeout()
    }
}

impl<C: ConnectionExt> CheckPragmas for DbConnection<C> {
    fn busy_timeout(&self) -> Result<i32, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            let BusyTimeout { timeout } =
                diesel::sql_query("PRAGMA busy_timeout").get_result::<BusyTimeout>(conn)?;
            Ok(timeout)
        })
    }
}
