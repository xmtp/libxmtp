use std::cell::{Ref, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use super::owned_row::OwnedSqliteRow;
use super::sqlite_value::{OwnedSqliteValue, SqliteValue};
use super::stmt::StatementUse;
use crate::WasmSqlite;
use diesel::{
    backend::Backend,
    row::{Field, IntoOwnedRow, PartialRow, Row, RowIndex, RowSealed},
};

#[allow(missing_debug_implementations)]
pub struct SqliteRow<'stmt, 'query> {
    pub(super) inner: Rc<RefCell<PrivateSqliteRow<'stmt, 'query>>>,
    pub(super) field_count: usize,
}

pub(super) enum PrivateSqliteRow<'stmt, 'query> {
    Direct(StatementUse<'stmt, 'query>),
    Duplicated {
        values: Vec<Option<OwnedSqliteValue>>,
        column_names: Rc<[Option<String>]>,
    },
}

impl<'stmt, 'query> IntoOwnedRow<'stmt, WasmSqlite> for SqliteRow<'stmt, 'query> {
    type OwnedRow = OwnedSqliteRow;

    type Cache = Option<Arc<[Option<String>]>>;

    fn into_owned(self, column_name_cache: &mut Self::Cache) -> Self::OwnedRow {
        self.inner.borrow().moveable(column_name_cache)
    }
}

impl<'stmt, 'query> PrivateSqliteRow<'stmt, 'query> {
    pub(super) fn duplicate(
        &mut self,
        column_names: &mut Option<Rc<[Option<String>]>>,
    ) -> PrivateSqliteRow<'stmt, 'query> {
        match self {
            PrivateSqliteRow::Direct(stmt) => {
                let column_names = if let Some(column_names) = column_names {
                    column_names.clone()
                } else {
                    let c: Rc<[Option<String>]> = Rc::from(
                        (0..stmt.column_count())
                            .map(|idx| stmt.field_name(idx).map(|s| s.to_owned()))
                            .collect::<Vec<_>>(),
                    );
                    *column_names = Some(c.clone());
                    c
                };
                PrivateSqliteRow::Duplicated {
                    values: (0..stmt.column_count())
                        .map(|idx| stmt.copy_value(idx))
                        .collect(),
                    column_names,
                }
            }
            PrivateSqliteRow::Duplicated {
                values,
                column_names,
            } => PrivateSqliteRow::Duplicated {
                values: values
                    .iter()
                    .map(|v| v.as_ref().map(|v| v.duplicate()))
                    .collect(),
                column_names: column_names.clone(),
            },
        }
    }

    pub(super) fn moveable(
        &self,
        column_name_cache: &mut Option<Arc<[Option<String>]>>,
    ) -> OwnedSqliteRow {
        match self {
            PrivateSqliteRow::Direct(stmt) => {
                if column_name_cache.is_none() {
                    *column_name_cache = Some(
                        (0..stmt.column_count())
                            .map(|idx| stmt.field_name(idx).map(|s| s.to_owned()))
                            .collect::<Vec<_>>()
                            .into(),
                    );
                }
                let column_names = Arc::clone(
                    column_name_cache
                        .as_ref()
                        .expect("This is initialized above"),
                );
                OwnedSqliteRow::new(
                    (0..stmt.column_count())
                        .map(|idx| stmt.copy_value(idx))
                        .collect(),
                    column_names,
                )
            }
            PrivateSqliteRow::Duplicated {
                values,
                column_names,
            } => {
                if column_name_cache.is_none() {
                    *column_name_cache = Some(
                        (*column_names)
                            .iter()
                            .map(|s| s.to_owned())
                            .collect::<Vec<_>>()
                            .into(),
                    );
                }
                let column_names = Arc::clone(
                    column_name_cache
                        .as_ref()
                        .expect("This is initialized above"),
                );
                OwnedSqliteRow::new(
                    values
                        .iter()
                        .map(|v| v.as_ref().map(|v| v.duplicate()))
                        .collect(),
                    column_names,
                )
            }
        }
    }
}

impl<'stmt, 'query> RowSealed for SqliteRow<'stmt, 'query> {}

impl<'stmt, 'query> Row<'stmt, WasmSqlite> for SqliteRow<'stmt, 'query> {
    type Field<'field> = SqliteField<'field, 'field> where 'stmt: 'field, Self: 'field;
    type InnerPartialRow = Self;

    fn field_count(&self) -> usize {
        self.field_count
    }

    fn get<'field, I>(&'field self, idx: I) -> Option<Self::Field<'field>>
    where
        'stmt: 'field,
        Self: RowIndex<I>,
    {
        let idx = self.idx(idx)?;
        Some(SqliteField {
            row: self.inner.borrow(),
            col_idx: i32::try_from(idx).ok()?,
        })
    }

    fn partial_row(&self, range: std::ops::Range<usize>) -> PartialRow<'_, Self::InnerPartialRow> {
        PartialRow::new(self, range)
    }
}

impl<'stmt, 'query> RowIndex<usize> for SqliteRow<'stmt, 'query> {
    fn idx(&self, idx: usize) -> Option<usize> {
        if idx < self.field_count {
            Some(idx)
        } else {
            None
        }
    }
}

impl<'stmt, 'idx, 'query> RowIndex<&'idx str> for SqliteRow<'stmt, 'query> {
    fn idx(&self, field_name: &'idx str) -> Option<usize> {
        match &mut *self.inner.borrow_mut() {
            PrivateSqliteRow::Direct(stmt) => stmt.index_for_column_name(field_name),
            PrivateSqliteRow::Duplicated { column_names, .. } => column_names
                .iter()
                .position(|n| n.as_ref().map(|s| s as &str) == Some(field_name)),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct SqliteField<'stmt, 'query> {
    pub(super) row: Ref<'stmt, PrivateSqliteRow<'stmt, 'query>>,
    pub(super) col_idx: i32,
}

impl<'stmt, 'query> Field<'stmt, WasmSqlite> for SqliteField<'stmt, 'query> {
    fn field_name(&self) -> Option<&str> {
        match &*self.row {
            PrivateSqliteRow::Direct(stmt) => stmt.field_name(self.col_idx),
            PrivateSqliteRow::Duplicated { column_names, .. } => column_names
                .get(self.col_idx as usize)
                .and_then(|t| t.as_ref().map(|n| n as &str)),
        }
    }

    fn is_null(&self) -> bool {
        self.value().is_none()
    }

    fn value(&self) -> Option<<WasmSqlite as Backend>::RawValue<'_>> {
        SqliteValue::new(Ref::clone(&self.row), self.col_idx)
    }
}
