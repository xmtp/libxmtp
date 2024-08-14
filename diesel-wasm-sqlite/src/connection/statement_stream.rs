use std::cell::RefCell;
use std::rc::Rc;

use super::row::{PrivateSqliteRow, SqliteRow};
use super::stmt::StatementUse;
use diesel::result::QueryResult;
use futures::stream::LocalBoxStream;
use futures::{Stream, TryStreamExt};

pub struct StatementStream<'stmt, 'quer> {
    stream: LocalBoxStream<'query, QueryResult<SqliteRow<'stmt, 'query>>>,
}

impl<'stmt, 'query> Stream for StatementStream<'stmt, 'query> {
    type Item = QueryResult<SqliteRow<'stmt, 'query>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let stream = &mut self.stream;
        stream.try_poll_next_unpin(cx)
    }
}

#[allow(missing_debug_implementations)]
pub struct PrivateStatementStream<'stmt, 'query> {
    inner: StatementStreamState<'stmt, 'query>,
    column_names: Option<Rc<[Option<String>]>>,
    field_count: usize,
}

impl<'stmt, 'query> PrivateStatementStream<'stmt, 'query> {
    #[cold]
    async fn handle_duplicated_row_case(
        outer_last_row: &mut Rc<RefCell<PrivateSqliteRow<'stmt, 'query>>>,
        column_names: &mut Option<Rc<[Option<String>]>>,
        field_count: usize,
    ) -> Option<QueryResult<SqliteRow<'stmt, 'query>>> {
        // We don't own the statement. There is another existing reference, likely because
        // a user stored the row in some long time container before calling next another time
        // In this case we copy out the current values into a temporary store and advance
        // the statement iterator internally afterwards
        let last_row = {
            let mut last_row = match outer_last_row.try_borrow_mut() {
                Ok(o) => o,
                Err(_e) => {
                    return Some(Err(diesel::result::Error::DeserializationError(
                                    "Failed to reborrow row. Try to release any `SqliteField` or `SqliteValue` \
                                     that exists at this point"
                                        .into(),
                                )));
                }
            };
            let last_row = &mut *last_row;
            let duplicated = last_row.duplicate(column_names);
            std::mem::replace(last_row, duplicated)
        };
        if let PrivateSqliteRow::Direct(mut stmt) = last_row {
            let res = stmt.step(false).await;
            *outer_last_row = Rc::new(RefCell::new(PrivateSqliteRow::Direct(stmt)));
            match res {
                Err(e) => Some(Err(e)),
                Ok(false) => None,
                Ok(true) => Some(Ok(SqliteRow {
                    inner: Rc::clone(outer_last_row),
                    field_count,
                })),
            }
        } else {
            // any other state than `PrivateSqliteRow::Direct` is invalid here
            // and should not happen. If this ever happens this is a logic error
            // in the code above
            unreachable!(
                "You've reached an impossible internal state. \
                             If you ever see this error message please open \
                             an issue at https://github.com/diesel-rs/diesel \
                             providing example code how to trigger this error."
            )
        }
    }
}

enum StatementStreamState<'stmt, 'query> {
    NotStarted(Option<StatementUse<'stmt, 'query>>),
    Started(Rc<RefCell<PrivateSqliteRow<'stmt, 'query>>>),
}

impl<'stmt, 'query> PrivateStatementStream<'stmt, 'query> {
    pub fn new(stmt: StatementUse<'stmt, 'query>) -> PrivateStatementStream<'stmt, 'query> {
        Self {
            inner: StatementStreamState::NotStarted(Some(stmt)),
            column_names: None,
            field_count: 0,
        }
    }
}
/// Rolling a custom `Stream` impl on PrivateStatementStream was taking too long/tricky
/// so using `futures::unfold`. Rolling a custom `Stream` would probably be better,
/// but performance wise/code-readability sense not very different
impl<'stmt, 'query> PrivateStatementStream<'stmt, 'query> {
    pub fn stream(self) -> LocalBoxStream<'query, QueryResult<SqliteRow<'stmt, 'query>>> {
        use StatementStreamState::{NotStarted, Started};
        let stream = futures::stream::unfold(self, |mut statement| async move {
            match statement.inner {
                NotStarted(mut stmt @ Some(_)) => {
                    let mut stmt = stmt
                        .take()
                        .expect("It must be there because we checked that above");
                    match stmt.step(true).await {
                        Ok(true) => {
                            let field_count = stmt.column_count() as usize;
                            statement.field_count = field_count;
                            let inner = Rc::new(RefCell::new(PrivateSqliteRow::Direct(stmt)));
                            let new_inner = inner.clone();
                            Some((
                                Ok(SqliteRow { inner, field_count }),
                                Self {
                                    inner: Started(new_inner),
                                    ..statement
                                },
                            ))
                        }
                        Ok(false) => None,
                        Err(e) => Some((
                            Err(e),
                            Self {
                                inner: NotStarted(Some(stmt)),
                                ..statement
                            },
                        )),
                    }
                    // res.poll_next(cx).map(|t| t.flatten())
                }
                Started(ref mut last_row) => {
                    // There was already at least one iteration step
                    // We check here if the caller already released the row value or not
                    // by checking if our Rc owns the data or not
                    if let Some(last_row_ref) = Rc::get_mut(last_row) {
                        // We own the statement, there is no other reference here.
                        // This means we don't need to copy out values from the sqlite provided
                        // datastructures for now
                        // We don't need to use the runtime borrowing system of the RefCell here
                        // as we have a mutable reference, so all of this below is checked at compile time
                        if let PrivateSqliteRow::Direct(ref mut stmt) = last_row_ref.get_mut() {
                            // This is actually safe here as we've already
                            // performed one step. For the first step we would have
                            // used `StatementStreamState::NotStarted` where we don't
                            // have access to `PrivateSqliteRow` at all
                            match stmt.step(false).await {
                                Err(e) => Some((
                                    Err(e),
                                    Self {
                                        inner: Started(Rc::clone(last_row)),
                                        ..statement
                                    },
                                )),
                                Ok(false) => None,
                                Ok(true) => {
                                    let field_count = statement.field_count;
                                    Some((
                                        Ok(SqliteRow {
                                            inner: Rc::clone(last_row),
                                            field_count,
                                        }),
                                        Self {
                                            inner: Started(Rc::clone(last_row)),
                                            ..statement
                                        },
                                    ))
                                }
                            }
                        } else {
                            // any other state than `PrivateSqliteRow::Direct` is invalid here
                            // and should not happen. If this ever happens this is a logic error
                            // in the code above
                            unreachable!(
                                "You've reached an impossible internal state. \
                             If you ever see this error message please open \
                             an issue at https://github.com/diesel-rs/diesel \
                             providing example code how to trigger this error."
                            )
                        }
                    } else {
                        let res = Self::handle_duplicated_row_case(
                            last_row,
                            &mut statement.column_names,
                            statement.field_count,
                        )
                        .await;
                        res.map(|r| {
                            (
                                r,
                                Self {
                                    inner: Started(Rc::clone(last_row)),
                                    ..statement
                                },
                            )
                        })
                    }
                }
                NotStarted(_s) => {
                    // we likely got an error while executing the other
                    // `NotStarted` branch above. In this case we just want to stop
                    // iterating here
                    None
                }
            }
        });
        Box::pin(stream)
    }
}
