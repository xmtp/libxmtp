use crate::connection::WasmSqliteConnection;
use crate::WasmSqlite;
use diesel::backend::Backend;
use diesel::debug_query;
use diesel::insertable::InsertValues;
use diesel::insertable::{CanInsertInSingleQuery, ColumnInsertValue, DefaultableColumnInsertValue};
use diesel::query_builder::{AstPass, QueryId, ValuesClause};
use diesel::query_builder::{BatchInsert, InsertStatement};
use diesel::query_builder::{DebugQuery, QueryFragment};
use diesel::{QueryResult, QuerySource, Table};
use diesel_async::methods::ExecuteDsl;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use futures::FutureExt;
use std::fmt::{self, Debug, Display};
/*
pub trait DebugQueryHelper<ContainsDefaultableValue> {
    fn fmt_debug(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
    fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<'a, T, V, QId, Op, Ret, const STATIC_QUERY_ID: bool> DebugQueryHelper<Yes>
    for DebugQuery<
        'a,
        InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op, Ret>,
        WasmSqlite,
    >
where
    V: QueryFragment<WasmSqlite>,
    T: Copy + QuerySource,
    Op: Copy,
    Ret: Copy,
    for<'b> InsertStatement<T, &'b ValuesClause<V, T>, Op, Ret>: QueryFragment<WasmSqlite>,
{
    fn fmt_debug(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut statements = vec![String::from("BEGIN")];
        for record in self.query.records.values.iter() {
            let stmt = InsertStatement::new(
                self.query.target,
                record,
                self.query.operator,
                self.query.returning,
            );
            statements.push(debug_query(&stmt).to_string());
        }
        statements.push("COMMIT".into());

        f.debug_struct("Query")
            .field("sql", &statements)
            .field("binds", &[] as &[i32; 0])
            .finish()
    }

    fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "BEGIN;")?;
        for record in self.query.records.values.iter() {
            let stmt = InsertStatement::new(
                self.query.target,
                record,
                self.query.operator,
                self.query.returning,
            );
            writeln!(f, "{}", debug_query(&stmt))?;
        }
        writeln!(f, "COMMIT;")?;
        Ok(())
    }
}

#[allow(unsafe_code)] // cast to transparent wrapper type
impl<'a, T, V, QId, Op, const STATIC_QUERY_ID: bool> DebugQueryHelper<No>
    for DebugQuery<'a, InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>, WasmSqlite>
where
    T: Copy + QuerySource,
    Op: Copy,
    DebugQuery<
        'a,
        InsertStatement<T, SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>, Op>,
        WasmSqlite,
    >: Debug + Display,
{
    fn fmt_debug(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = unsafe {
            // This cast is safe as `SqliteBatchInsertWrapper` is #[repr(transparent)]
            &*(self as *const DebugQuery<
                'a,
                InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>,
                WasmSqlite,
            >
                as *const DebugQuery<
                    'a,
                    InsertStatement<T, SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>, Op>,
                    WasmSqlite,
                >)
        };
        <_ as Debug>::fmt(value, f)
    }

    fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = unsafe {
            // This cast is safe as `SqliteBatchInsertWrapper` is #[repr(transparent)]
            &*(self as *const DebugQuery<
                'a,
                InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>,
                WasmSqlite,
            >
                as *const DebugQuery<
                    'a,
                    InsertStatement<T, SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>, Op>,
                    WasmSqlite,
                >)
        };
        <_ as Display>::fmt(value, f)
    }
}

impl<'a, T, V, QId, Op, O, const STATIC_QUERY_ID: bool> Display
    for DebugQuery<
        'a,
        InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op>,
        WasmSqlite,
    >
where
    T: QuerySource,
    V: ContainsDefaultableValue<Out = O>,
    Self: DebugQueryHelper<O>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_display(f)
    }
}

impl<'a, T, V, QId, Op, O, const STATIC_QUERY_ID: bool> Debug
    for DebugQuery<
        'a,
        InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op>,
        WasmSqlite,
    >
where
    T: QuerySource,
    V: ContainsDefaultableValue<Out = O>,
    Self: DebugQueryHelper<O>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_debug(f)
    }
}
*/
#[allow(missing_debug_implementations, missing_copy_implementations)]
pub struct Yes;

impl Default for Yes {
    fn default() -> Self {
        Yes
    }
}

#[allow(missing_debug_implementations, missing_copy_implementations)]
pub struct No;

impl Default for No {
    fn default() -> Self {
        No
    }
}

pub trait Any<Rhs> {
    type Out: Any<Yes> + Any<No>;
}

impl Any<No> for No {
    type Out = No;
}

impl Any<Yes> for No {
    type Out = Yes;
}

impl Any<No> for Yes {
    type Out = Yes;
}

impl Any<Yes> for Yes {
    type Out = Yes;
}

pub trait ContainsDefaultableValue {
    type Out: Any<Yes> + Any<No>;
}

impl<C, B> ContainsDefaultableValue for ColumnInsertValue<C, B> {
    type Out = No;
}

impl<I> ContainsDefaultableValue for DefaultableColumnInsertValue<I> {
    type Out = Yes;
}

impl<I, const SIZE: usize> ContainsDefaultableValue for [I; SIZE]
where
    I: ContainsDefaultableValue,
{
    type Out = I::Out;
}

impl<I, T> ContainsDefaultableValue for ValuesClause<I, T>
where
    I: ContainsDefaultableValue,
{
    type Out = I::Out;
}

impl<'a, T> ContainsDefaultableValue for &'a T
where
    T: ContainsDefaultableValue,
{
    type Out = T::Out;
}

impl<V, T, QId, Op, O, const STATIC_QUERY_ID: bool> ExecuteDsl<WasmSqliteConnection, WasmSqlite>
    for InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op>
where
    T: QuerySource,
    V: ContainsDefaultableValue<Out = O>,
    O: Default,
    (O, Self): ExecuteDsl<WasmSqliteConnection, WasmSqlite>,
{
    fn execute<'conn, 'query>(
        query: Self,
        conn: &'conn mut WasmSqliteConnection,
    ) -> <WasmSqliteConnection as AsyncConnection>::ExecuteFuture<'conn, 'query> {
        <(O, Self) as ExecuteDsl<WasmSqliteConnection, WasmSqlite>>::execute(
            (O::default(), query),
            conn,
        )
    }
}

impl<V, T, QId, Op, const STATIC_QUERY_ID: bool> ExecuteDsl<WasmSqliteConnection, WasmSqlite>
    for (
        Yes,
        InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op>,
    )
where
    T: Table + Copy + QueryId + 'static,
    T::FromClause: QueryFragment<WasmSqlite>,
    Op: Copy + QueryId + QueryFragment<WasmSqlite> + 'static,
    V: InsertValues<WasmSqlite, T> + CanInsertInSingleQuery<WasmSqlite> + QueryId + 'static,
{
    fn execute<'conn, 'query>(
        (Yes, query): Self,
        conn: &'conn mut WasmSqliteConnection,
    ) -> <WasmSqliteConnection as AsyncConnection>::ExecuteFuture<'conn, 'query>
    where
        Self: 'query,
    {
        conn.transaction(move |conn| {
            async move {
                let mut result = 0;
                for record in &query.records.values {
                    let stmt =
                        InsertStatement::new(query.target, record, query.operator, query.returning);
                    result += stmt.execute(conn).await?;
                }
                Ok(result)
            }
            .scope_boxed_local()
        })
    }
}

#[allow(missing_debug_implementations, missing_copy_implementations)]
#[repr(transparent)]
pub struct SqliteBatchInsertWrapper<V, T, QId, const STATIC_QUERY_ID: bool>(
    BatchInsert<V, T, QId, STATIC_QUERY_ID>,
);

impl<V, Tab, QId, const STATIC_QUERY_ID: bool> QueryFragment<WasmSqlite>
    for SqliteBatchInsertWrapper<Vec<ValuesClause<V, Tab>>, Tab, QId, STATIC_QUERY_ID>
where
    ValuesClause<V, Tab>: QueryFragment<WasmSqlite>,
    V: QueryFragment<WasmSqlite>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        if !STATIC_QUERY_ID {
            out.unsafe_to_cache_prepared();
        }

        let mut values = self.0.values.iter();
        if let Some(value) = values.next() {
            value.walk_ast(out.reborrow())?;
        }
        for value in values {
            out.push_sql(", (");
            value.values.walk_ast(out.reborrow())?;
            out.push_sql(")");
        }
        Ok(())
    }
}

#[allow(missing_copy_implementations, missing_debug_implementations)]
#[repr(transparent)]
pub struct SqliteCanInsertInSingleQueryHelper<T: ?Sized>(T);

impl<V, T, QId, const STATIC_QUERY_ID: bool> CanInsertInSingleQuery<WasmSqlite>
    for SqliteBatchInsertWrapper<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>
where
    // We constrain that here on an internal helper type
    // to make sure that this does not accidentally leak
    // so that none does really implement normal batch
    // insert for inserts with default values here
    SqliteCanInsertInSingleQueryHelper<V>: CanInsertInSingleQuery<WasmSqlite>,
{
    fn rows_to_insert(&self) -> Option<usize> {
        Some(self.0.values.len())
    }
}

impl<T> CanInsertInSingleQuery<WasmSqlite> for SqliteCanInsertInSingleQueryHelper<T>
where
    T: CanInsertInSingleQuery<WasmSqlite>,
{
    fn rows_to_insert(&self) -> Option<usize> {
        self.0.rows_to_insert()
    }
}

impl<V, T, QId, const STATIC_QUERY_ID: bool> QueryId
    for SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>
where
    BatchInsert<V, T, QId, STATIC_QUERY_ID>: QueryId,
{
    type QueryId = <BatchInsert<V, T, QId, STATIC_QUERY_ID> as QueryId>::QueryId;

    const HAS_STATIC_QUERY_ID: bool =
        <BatchInsert<V, T, QId, STATIC_QUERY_ID> as QueryId>::HAS_STATIC_QUERY_ID;
}

impl<V, T, QId, Op, const STATIC_QUERY_ID: bool> ExecuteDsl<WasmSqliteConnection, WasmSqlite>
    for (
        No,
        InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>,
    )
where
    T: Table + QueryId + 'static,
    T::FromClause: QueryFragment<WasmSqlite>,
    Op: QueryFragment<WasmSqlite> + QueryId,
    SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>:
        QueryFragment<WasmSqlite> + QueryId + CanInsertInSingleQuery<WasmSqlite>,
{
    fn execute<'conn, 'query>(
        (No, query): Self,
        conn: &'conn mut WasmSqliteConnection,
    ) -> <WasmSqliteConnection as AsyncConnection>::ExecuteFuture<'conn, 'query> {
        let query = InsertStatement::new(
            query.target,
            SqliteBatchInsertWrapper(query.records),
            query.operator,
            query.returning,
        );
        query.execute(conn)
    }
}

macro_rules! tuple_impls {
        ($(
            $Tuple:tt {
                $(($idx:tt) -> $T:ident, $ST:ident, $TT:ident,)+
            }
        )+) => {
            $(
                impl_contains_defaultable_value!($($T,)*);
            )*
        }
    }

macro_rules! impl_contains_defaultable_value {
      (
        @build
        start_ts = [$($ST: ident,)*],
        ts = [$T1: ident,],
        bounds = [$($bounds: tt)*],
        out = [$($out: tt)*],
    )=> {
        impl<$($ST,)*> ContainsDefaultableValue for ($($ST,)*)
        where
            $($ST: ContainsDefaultableValue,)*
            $($bounds)*
            $T1::Out: Any<$($out)*>,
        {
            type Out = <$T1::Out as Any<$($out)*>>::Out;
        }

    };
    (
        @build
        start_ts = [$($ST: ident,)*],
        ts = [$T1: ident, $($T: ident,)+],
        bounds = [$($bounds: tt)*],
        out = [$($out: tt)*],
    )=> {
        impl_contains_defaultable_value! {
            @build
            start_ts = [$($ST,)*],
            ts = [$($T,)*],
            bounds = [$($bounds)* $T1::Out: Any<$($out)*>,],
            out = [<$T1::Out as Any<$($out)*>>::Out],
        }
    };
    ($T1: ident, $($T: ident,)+) => {
        impl_contains_defaultable_value! {
            @build
            start_ts = [$T1, $($T,)*],
            ts = [$($T,)*],
            bounds = [],
            out = [$T1::Out],
        }
    };
    ($T1: ident,) => {
        impl<$T1> ContainsDefaultableValue for ($T1,)
        where $T1: ContainsDefaultableValue,
        {
            type Out = <$T1 as ContainsDefaultableValue>::Out;
        }
    }
}

diesel_derives::__diesel_for_each_tuple!(tuple_impls);
