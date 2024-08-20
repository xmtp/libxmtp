use crate::WasmSqlite;
use diesel::{
    insertable::{ColumnInsertValue, DefaultableColumnInsertValue, InsertValues},
    query_builder::AstPass,
    query_builder::NoFromClause,
    query_builder::QueryFragment,
    AppearsOnTable, Column, Expression, QueryResult,
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
