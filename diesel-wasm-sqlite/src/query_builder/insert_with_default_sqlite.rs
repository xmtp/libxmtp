use crate::{WasmSqlite, dsl::ExecuteDsl};
use diesel::connection::Connection;
use diesel::insertable::InsertValues;
use diesel::insertable::{CanInsertInSingleQuery, ColumnInsertValue, DefaultableColumnInsertValue};
// use diesel::prelude::RunQueryDsl;
use diesel::query_builder::QueryFragment;
use diesel::query_builder::{AstPass, QueryId, ValuesClause};
use diesel::query_builder::{BatchInsert, InsertStatement};
// use diesel::query_dsl::load_dsl::ExecuteDsl;
use diesel::{QueryResult, QuerySource, Table};

#[cfg(any(feature = "unsafe-debug-query", test))]
pub mod unsafe_debug_query {
    use super::*;
    use diesel::backend::Backend;
    use diesel::{debug_query, query_builder::DebugQuery};
    use std::fmt::{self, Debug, Display};

    pub trait DebugQueryHelper<ContainsDefaultableValue> {
        fn fmt_debug(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
        fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
    }

    // FIXME: Here to temporarily workaround private fields of `DebugQuery`.
    // this should never go in prod
    // this is cause `DebugQuery` is private
    #[repr(transparent)]
    struct DebugQueryUnsafe<'a, T: 'a, DB> {
        pub(crate) query: &'a T,
        _marker: std::marker::PhantomData<DB>,
    }

    impl<'a, T, V, QId, Op, Ret, const STATIC_QUERY_ID: bool> DebugQueryHelper<Yes>
        for DebugQuery<
            'a,
            InsertStatement<
                T,
                BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>,
                Op,
                Ret,
            >,
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
            let query = unsafe {
                std::mem::transmute::<
                    &DebugQuery<
                        'a,
                        InsertStatement<
                            T,
                            BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>,
                            Op,
                            Ret,
                        >,
                        WasmSqlite,
                    >,
                    &DebugQueryUnsafe<
                        'a,
                        InsertStatement<
                            T,
                            BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>,
                            Op,
                            Ret,
                        >,
                        WasmSqlite,
                    >,
                >(self)
            };
            let query = query.query;
            let mut statements = vec![String::from("BEGIN")];
            for record in query.records.values.iter() {
                let stmt =
                    InsertStatement::new(query.target, record, query.operator, query.returning);
                statements.push(debug_query(&stmt).to_string());
            }
            statements.push("COMMIT".into());
            f.debug_struct("Query")
                .field("sql", &statements)
                .field("binds", &[] as &[i32; 0])
                .finish()
        }

        fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let query = unsafe {
                std::mem::transmute::<
                    &DebugQuery<
                        'a,
                        InsertStatement<
                            T,
                            BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>,
                            Op,
                            Ret,
                        >,
                        WasmSqlite,
                    >,
                    &DebugQueryUnsafe<
                        'a,
                        InsertStatement<
                            T,
                            BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>,
                            Op,
                            Ret,
                        >,
                        WasmSqlite,
                    >,
                >(self)
            };
            let query = query.query;
            writeln!(f, "BEGIN;")?;
            for record in query.records.values.iter() {
                let stmt =
                    InsertStatement::new(query.target, record, query.operator, query.returning);
                writeln!(f, "{}", debug_query(&stmt))?;
            }
            writeln!(f, "COMMIT;")?;
            Ok(())
        }
    }

    #[allow(unsafe_code)] // cast to transparent wrapper type
    impl<'a, T, V, QId, Op, const STATIC_QUERY_ID: bool> DebugQueryHelper<No>
        for DebugQuery<
            'a,
            InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>,
            WasmSqlite,
        >
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
                        InsertStatement<
                            T,
                            SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>,
                            Op,
                        >,
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
                        InsertStatement<
                            T,
                            SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>,
                            Op,
                        >,
                        WasmSqlite,
                    >)
            };
            <_ as Display>::fmt(value, f)
        }
    }

    pub struct DebugQueryWrapper<'a, T: 'a, DB>(DebugQuery<'a, T, DB>);

    impl<'a, T, DB> DebugQueryWrapper<'a, T, DB> {
        pub fn new(query: &'a T) -> Self {
            DebugQueryWrapper(diesel::debug_query(query))
        }
    }

    impl<'a, T, DB> Display for DebugQueryWrapper<'a, T, DB>
    where
        DB: Backend + Default,
        DB::QueryBuilder: Default,
        T: QueryFragment<DB>,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            std::fmt::Display::fmt(&self.0, f)
        }
    }

    impl<'a, T, DB> Debug for DebugQueryWrapper<'a, T, DB>
    where
        DB: Backend + Default,
        DB::QueryBuilder: Default,
        T: QueryFragment<DB>,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            std::fmt::Debug::fmt(&self.0, f)
        }
    }

    impl<'a, T, V, QId, Op, Ret, const STATIC_QUERY_ID: bool> DebugQueryHelper<Yes>
        for DebugQueryWrapper<
            'a,
            InsertStatement<
                T,
                BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>,
                Op,
                Ret,
            >,
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
            DebugQueryHelper::fmt_debug(&self.0, f)
        }

        fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            DebugQueryHelper::fmt_display(&self.0, f)
        }
    }

    impl<'a, T, V, QId, Op, const STATIC_QUERY_ID: bool> DebugQueryHelper<No>
        for DebugQueryWrapper<
            'a,
            InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>,
            WasmSqlite,
        >
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
            DebugQueryHelper::fmt_debug(&self.0, f)
        }

        fn fmt_display(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            DebugQueryHelper::fmt_display(&self.0, f)
        }
    }

    impl<'a, T, V, QId, Op, O, const STATIC_QUERY_ID: bool> Display
        for DebugQueryWrapper<
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
        for DebugQueryWrapper<
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
}

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

impl<V, T, QId, C, Op, O, const STATIC_QUERY_ID: bool> ExecuteDsl<C, WasmSqlite>
    for InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op>
where
    T: QuerySource,
    C: Connection<Backend = WasmSqlite>,
    V: ContainsDefaultableValue<Out = O>,
    O: Default,
    (O, Self): ExecuteDsl<C, WasmSqlite>,
{
    fn execute(query: Self, conn: &mut C) -> QueryResult<usize> {
        <(O, Self) as ExecuteDsl<C, WasmSqlite>>::execute(
            (O::default(), query),
            conn,
        )
    }
}

impl<V, T, QId, C, Op, const STATIC_QUERY_ID: bool> ExecuteDsl<C, WasmSqlite>
    for (
        Yes,
        InsertStatement<T, BatchInsert<Vec<ValuesClause<V, T>>, T, QId, STATIC_QUERY_ID>, Op>,
    )
where
    T: Table + Copy + QueryId + 'static,
    C: Connection<Backend = WasmSqlite>,
    T::FromClause: QueryFragment<WasmSqlite>,
    Op: Copy + QueryId + QueryFragment<WasmSqlite>,
    V: InsertValues<WasmSqlite, T> + CanInsertInSingleQuery<WasmSqlite> + QueryId,
{
    fn execute((Yes, query): Self, conn: &mut C) -> QueryResult<usize> {
        conn.transaction(|conn| {
            let mut result = 0;
            for record in &query.records.values {
                let stmt =
                    InsertStatement::new(query.target, record, query.operator, query.returning);
                result += ExecuteDsl::<C, WasmSqlite>::execute(stmt, conn)?;
            }
            Ok(result)
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

impl<V, T, QId, C, Op, const STATIC_QUERY_ID: bool> ExecuteDsl<C, WasmSqlite>
    for (
        No,
        InsertStatement<T, BatchInsert<V, T, QId, STATIC_QUERY_ID>, Op>,
    )
where
    T: Table + QueryId + 'static,
    T::FromClause: QueryFragment<WasmSqlite>,
    C: Connection<Backend = WasmSqlite>,
    Op: QueryFragment<WasmSqlite> + QueryId,
    SqliteBatchInsertWrapper<V, T, QId, STATIC_QUERY_ID>:
        QueryFragment<WasmSqlite> + QueryId + CanInsertInSingleQuery<WasmSqlite>,
{
    fn execute((No, query): Self, conn: &mut C) -> QueryResult<usize> {
        let query = InsertStatement::new(
            query.target,
            SqliteBatchInsertWrapper(query.records),
            query.operator,
            query.returning,
        );
        ExecuteDsl::<C, WasmSqlite>::execute(query, conn)
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
