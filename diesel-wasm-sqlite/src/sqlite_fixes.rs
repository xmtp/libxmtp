use crate::{connection::WasmSqliteConnection, WasmSqlite};
use diesel::{
    associations::HasTable,
    dsl::{Find, Update},
    expression::{is_aggregate, MixedAggregates, ValidGrouping},
    insertable::{ColumnInsertValue, DefaultableColumnInsertValue, InsertValues},
    prelude::{AsChangeset, Identifiable},
    query_builder::{
        AstPass, InsertOrIgnore, IntoUpdateTarget, NoFromClause, QueryFragment, Replace,
    },
    query_dsl::{
        methods::{ExecuteDsl, FindDsl, LoadQuery},
        UpdateAndFetchResults,
    },
    AppearsOnTable, Column, Expression, QueryResult, RunQueryDsl, Table,
};

/// We re-define Dsl traits to make `insert_with_default_sqlite.rs` generic over all `Connection`
/// implementations with `WasmSqlite` backend`. This works around Rusts orphan rules.
pub mod dsl {
    use diesel::{
        backend::Backend,
        dsl::Limit,
        query_builder::{QueryFragment, QueryId},
        query_dsl::methods::{LimitDsl, LoadQuery},
        Connection, QueryResult,
    };

    pub trait ExecuteDsl<
        Conn: Connection<Backend = DB>,
        DB: Backend = <Conn as Connection>::Backend,
    >: Sized
    {
        fn execute(query: Self, conn: &mut Conn) -> QueryResult<usize>;
    }

    impl<Conn, DB, T> ExecuteDsl<Conn, DB> for T
    where
        Conn: Connection<Backend = DB>,
        DB: Backend,
        T: QueryFragment<DB> + QueryId,
    {
        fn execute(query: T, conn: &mut Conn) -> QueryResult<usize> {
            conn.execute_returning_count(&query)
        }
    }

    pub trait RunQueryDsl<Conn>: Sized + diesel::query_dsl::RunQueryDsl<Conn> {
        fn execute(self, conn: &mut Conn) -> QueryResult<usize>
        where
            Conn: Connection,
            Self: ExecuteDsl<Conn>,
        {
            ExecuteDsl::execute(self, conn)
        }

        fn load<'query, U>(self, conn: &mut Conn) -> QueryResult<Vec<U>>
        where
            Self: LoadQuery<'query, Conn, U>,
        {
            <Self as diesel::query_dsl::RunQueryDsl<Conn>>::load(self, conn)
        }
        fn load_iter<'conn, 'query: 'conn, U, B>(
            self,
            conn: &'conn mut Conn,
        ) -> QueryResult<Self::RowIter<'conn>>
        where
            U: 'conn,
            Self: LoadQuery<'query, Conn, U, B> + 'conn,
        {
            <Self as diesel::query_dsl::RunQueryDsl<Conn>>::load_iter(self, conn)
        }
        fn get_result<'query, U>(self, conn: &mut Conn) -> QueryResult<U>
        where
            Self: LoadQuery<'query, Conn, U>,
        {
            <Self as diesel::query_dsl::RunQueryDsl<Conn>>::get_result(self, conn)
        }
        fn get_results<'query, U>(self, conn: &mut Conn) -> QueryResult<Vec<U>>
        where
            Self: LoadQuery<'query, Conn, U>,
        {
            <Self as diesel::query_dsl::RunQueryDsl<Conn>>::get_results(self, conn)
        }
        fn first<'query, U>(self, conn: &mut Conn) -> QueryResult<U>
        where
            Self: LimitDsl,
            Limit<Self>: LoadQuery<'query, Conn, U>,
        {
            <Self as diesel::query_dsl::RunQueryDsl<Conn>>::first(self, conn)
        }
    }

    impl<T, Conn> RunQueryDsl<Conn> for T where T: diesel::query_dsl::RunQueryDsl<Conn> {}
}

impl<Col, Expr> InsertValues<WasmSqlite, Col::Table>
    for DefaultableColumnInsertValue<ColumnInsertValue<Col, Expr>>
where
    Col: Column,
    Expr: Expression<SqlType = Col::SqlType> + AppearsOnTable<NoFromClause>,
    Self: QueryFragment<WasmSqlite>,
{
    fn column_names(&self, mut out: AstPass<'_, '_, WasmSqlite>) -> QueryResult<()> {
        if let Self::Expression(..) = *self {
            out.push_identifier(Col::NAME)?;
        }
        Ok(())
    }
}

impl<Col, Expr>
    QueryFragment<
        WasmSqlite,
        diesel::backend::sql_dialect::default_keyword_for_insert::DoesNotSupportDefaultKeyword,
    > for DefaultableColumnInsertValue<ColumnInsertValue<Col, Expr>>
where
    Expr: QueryFragment<WasmSqlite>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        if let Self::Expression(ref inner) = *self {
            inner.walk_ast(out.reborrow())?;
        }
        Ok(())
    }
}

impl QueryFragment<WasmSqlite> for InsertOrIgnore {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        out.push_sql("INSERT OR IGNORE");
        Ok(())
    }
}

impl QueryFragment<WasmSqlite> for Replace {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        out.push_sql("REPLACE");
        Ok(())
    }
}

impl<'b, Changes, Output> UpdateAndFetchResults<Changes, Output> for WasmSqliteConnection
where
    Changes: Copy + Identifiable,
    Changes: AsChangeset<Target = <Changes as HasTable>::Table> + IntoUpdateTarget,
    Changes::Table: FindDsl<Changes::Id>,
    Update<Changes, Changes>: ExecuteDsl<WasmSqliteConnection>,
    Find<Changes::Table, Changes::Id>: LoadQuery<'b, WasmSqliteConnection, Output>,
    <Changes::Table as Table>::AllColumns: ValidGrouping<()>,
    <<Changes::Table as Table>::AllColumns as ValidGrouping<()>>::IsAggregate:
        MixedAggregates<is_aggregate::No, Output = is_aggregate::No>,
{
    fn update_and_fetch(&mut self, changeset: Changes) -> QueryResult<Output> {
        diesel::update(changeset).set(changeset).execute(self)?;
        Changes::table().find(changeset.id()).get_result(self)
    }
}

mod parenthesis_wrapper {
    use super::*;

    use crate::WasmSqlite;
    // use diesel::query_builder::combination_clause::SupportsCombinationClause;
    use diesel::query_builder::{
        All, AstPass, Distinct, Except, Intersect, ParenthesisWrapper, QueryFragment,
        SupportsCombinationClause, Union,
    };

    impl<T: QueryFragment<WasmSqlite>> QueryFragment<WasmSqlite> for ParenthesisWrapper<T> {
        fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
            // SQLite does not support parenthesis around this clause
            // we can emulate this by construct a fake outer
            // SELECT * FROM (inner_query) statement
            out.push_sql("SELECT * FROM (");
            self.inner.walk_ast(out.reborrow())?;
            out.push_sql(")");
            Ok(())
        }
    }

    impl SupportsCombinationClause<Union, Distinct> for WasmSqlite {}
    impl SupportsCombinationClause<Union, All> for WasmSqlite {}
    impl SupportsCombinationClause<Intersect, Distinct> for WasmSqlite {}
    impl SupportsCombinationClause<Except, Distinct> for WasmSqlite {}
}

// Anything commented here are implementation present in diesel
// but not possible because parts of it exist as private types in diesel.

/*
impl AsExpression<TimestamptzSqlite> for now {
    type Expression = Coerce<now, TimestamptzSqlite>;

    fn as_expression(self) -> Self::Expression {
        Coerce::new(self)
    }
}

impl AsExpression<Nullable<TimestamptzSqlite>> for now {
    type Expression = Coerce<now, Nullable<TimestamptzSqlite>>;

    fn as_expression(self) -> Self::Expression {
        Coerce::new(self)
    }
}
*/

/*
use diesel::dsl;
use diesel::expression::grouped::Grouped;
use diesel::expression::AsExpression;
use diesel::operators::*;
use diesel::sql_types::SqlType;

/// Sqlite specific methods which are present on all expressions.
pub trait SqliteExpressionMethods: Expression + Sized {
    /// Creates a Sqlite `IS` expression.
    ///
    /// The `IS` operator work like = except when one or both of the operands are NULL.
    /// In this case, if both operands are NULL, then the `IS` operator evaluates to true.
    /// If one operand is NULL and the other is not, then the `IS` operator evaluates to false.
    /// It is not possible for an `IS` expression to evaluate to NULL.
    ///
    /// # Example
    ///
    /// ```rust
    /// # include!("../../doctest_setup.rs");
    /// #
    /// # fn main() {
    /// #     run_test().unwrap();
    /// # }
    /// #
    /// # fn run_test() -> QueryResult<()> {
    /// #     use schema::animals::dsl::*;
    /// #     let connection = &mut establish_connection();
    /// let jack_is_a_dog = animals
    ///     .select(name)
    ///     .filter(species.is("dog"))
    ///     .get_results::<Option<String>>(connection)?;
    /// assert_eq!(vec![Some("Jack".to_string())], jack_is_a_dog);
    /// #     Ok(())
    /// # }
    /// ```
    fn is<T>(self, other: T) -> dsl::Is<Self, T>
    where
        Self::SqlType: SqlType,
        T: AsExpression<Self::SqlType>,
    {
        Grouped(Is::new(self, other.as_expression()))
    }

    /// Creates a Sqlite `IS NOT` expression.
    ///
    /// The `IS NOT` operator work like != except when one or both of the operands are NULL.
    /// In this case, if both operands are NULL, then the `IS NOT` operator evaluates to false.
    /// If one operand is NULL and the other is not, then the `IS NOT` operator is true.
    /// It is not possible for an `IS NOT` expression to evaluate to NULL.
    ///
    /// # Example
    ///
    /// ```rust
    /// # include!("../../doctest_setup.rs");
    /// #
    /// # fn main() {
    /// #     run_test().unwrap();
    /// # }
    /// #
    /// # fn run_test() -> QueryResult<()> {
    /// #     use schema::animals::dsl::*;
    /// #     let connection = &mut establish_connection();
    /// let jack_is_not_a_spider = animals
    ///     .select(name)
    ///     .filter(species.is_not("spider"))
    ///     .get_results::<Option<String>>(connection)?;
    /// assert_eq!(vec![Some("Jack".to_string())], jack_is_not_a_spider);
    /// #     Ok(())
    /// # }
    /// ```
    #[allow(clippy::wrong_self_convention)] // This is named after the sql operator
    fn is_not<T>(self, other: T) -> dsl::IsNot<Self, T>
    where
        Self::SqlType: SqlType,
        T: AsExpression<Self::SqlType>,
    {
        Grouped(IsNot::new(self, other.as_expression()))
    }
}

impl<T: Expression> SqliteExpressionMethods for T {}
*/
