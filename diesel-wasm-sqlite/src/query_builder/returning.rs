use crate::backend::{SqliteReturningClause, WasmSqlite};
use diesel::query_builder::ReturningClause;
use diesel::query_builder::{AstPass, QueryFragment};
use diesel::result::QueryResult;

impl<Expr> QueryFragment<WasmSqlite, SqliteReturningClause> for ReturningClause<Expr>
where
    Expr: QueryFragment<WasmSqlite>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        out.skip_from(true);
        out.push_sql(" RETURNING ");
        self.0.walk_ast(out.reborrow())?;
        Ok(())
    }
}
