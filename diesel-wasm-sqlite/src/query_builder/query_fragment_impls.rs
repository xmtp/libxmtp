use crate::WasmSqlite;
use diesel::query_builder::into_conflict_clause::OnConflictSelectWrapper;
use diesel::query_builder::where_clause::BoxedWhereClause;
use diesel::query_builder::where_clause::WhereClause;
use diesel::query_builder::AstPass;
use diesel::query_builder::BoxedSelectStatement;
use diesel::query_builder::QueryFragment;
use diesel::query_builder::SelectStatement;
use diesel::result::QueryResult;
use diesel::QueryId;

// The corresponding impl for`NoWhereClause` is missing because of
// https://www.sqlite.org/lang_UPSERT.html (Parsing Ambiguity)
impl<F, S, D, W, O, LOf, G, H, LC> QueryFragment<WasmSqlite>
    for OnConflictSelectWrapper<SelectStatement<F, S, D, WhereClause<W>, O, LOf, G, H, LC>>
where
    SelectStatement<F, S, D, WhereClause<W>, O, LOf, G, H, LC>: QueryFragment<WasmSqlite>,
{
    fn walk_ast<'b>(&'b self, out: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        self.0.walk_ast(out)
    }
}

impl<'a, ST, QS, GB> QueryFragment<WasmSqlite>
    for OnConflictSelectWrapper<BoxedSelectStatement<'a, ST, QS, WasmSqlite, GB>>
where
    BoxedSelectStatement<'a, ST, QS, WasmSqlite, GB>: QueryFragment<WasmSqlite>,
    QS: QueryFragment<WasmSqlite>,
{
    fn walk_ast<'b>(&'b self, pass: AstPass<'_, 'b, WasmSqlite>) -> QueryResult<()> {
        // https://www.sqlite.org/lang_UPSERT.html (Parsing Ambiguity)
        self.0.build_query(pass, |where_clause, mut pass| {
            match where_clause {
                BoxedWhereClause::None => pass.push_sql(" WHERE 1=1 "),
                w => w.walk_ast(pass.reborrow())?,
            }
            Ok(())
        })
    }
}
