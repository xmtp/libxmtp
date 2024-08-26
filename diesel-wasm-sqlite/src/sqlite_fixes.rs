use crate::WasmSqlite;
use diesel::{
    insertable::{ColumnInsertValue, DefaultableColumnInsertValue, InsertValues},
    query_builder::AstPass,
    query_builder::NoFromClause,
    query_builder::QueryFragment,
    AppearsOnTable, Column, Expression, QueryId, QueryResult,
};

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

#[derive(Debug, Copy, Clone, QueryId)]
pub struct InsertOrIgnore;

impl QueryFragment<WasmSqlite> for InsertOrIgnore {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        out.push_sql("INSERT OR IGNORE");
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, QueryId)]
pub struct Replace;

impl QueryFragment<WasmSqlite> for Replace {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        out.push_sql("REPLACE");
        Ok(())
    }
}

mod parenthesis_wrapper {
    use super::*;

    use crate::WasmSqlite;
    // use diesel::query_builder::combination_clause::SupportsCombinationClause;
    use diesel::query_builder::{AstPass, QueryFragment};

    #[derive(Debug, Copy, Clone, QueryId)]
    /// Wrapper used to wrap rhs sql in parenthesis when supported by backend
    pub struct ParenthesisWrapper<T>(T);

    #[derive(Debug, Copy, Clone, QueryId)]
    /// Keep duplicate rows in the result
    pub struct All;

    impl QueryFragment<WasmSqlite> for All {
        fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
            out.push_sql("ALL ");
            Ok(())
        }
    }

    impl<T: QueryFragment<WasmSqlite>> QueryFragment<WasmSqlite> for ParenthesisWrapper<T> {
        fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
            // SQLite does not support parenthesis around this clause
            // we can emulate this by construct a fake outer
            // SELECT * FROM (inner_query) statement
            out.push_sql("SELECT * FROM (");
            self.0.walk_ast(out.reborrow())?;
            out.push_sql(")");
            Ok(())
        }
    }
    /*
    impl SupportsCombinationClause<Union, Distinct> for WasmSqlite {}
    impl SupportsCombinationClause<Union, All> for WasmSqlite {}
    impl SupportsCombinationClause<Intersect, Distinct> for WasmSqlite {}
    impl SupportsCombinationClause<Except, Distinct> for WasmSqlite {}
    */
}

// Anything commented here are implementation present in diesel
// but not possible because parts of it exist as private types in diesel.

/*
impl<'b, Changes, Output> UpdateAndFetchResults<Changes, Output> for SqliteConnection
where
    Changes: Copy + Identifiable,
    Changes: AsChangeset<Target = <Changes as HasTable>::Table> + IntoUpdateTarget,
    Changes::Table: FindDsl<Changes::Id>,
    Update<Changes, Changes>: ExecuteDsl<SqliteConnection>,
    Find<Changes::Table, Changes::Id>: LoadQuery<'b, SqliteConnection, Output>,
    <Changes::Table as Table>::AllColumns: ValidGrouping<()>,
    <<Changes::Table as Table>::AllColumns as ValidGrouping<()>>::IsAggregate:
        MixedAggregates<is_aggregate::No, Output = is_aggregate::No>,
{
    fn update_and_fetch(&mut self, changeset: Changes) -> QueryResult<Output> {
        crate::update(changeset).set(changeset).execute(self)?;
        Changes::table().find(changeset.id()).get_result(self)
    }
}
*/
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
