//! Validators run after ops + the final sync. They check that all clients
//! converged (no forks, no missing messages).
//!
//! Each validator self-registers via `inventory::submit!` with a `&'static`
//! reference to its impl, same shape as `ops::OpEntry`.

use crate::app::health::context::HealthContext;
use crate::app::health::registry::{self, Named, RegistryEntry};
use crate::app::health::result::OpResult;
use async_trait::async_trait;

mod no_forks;
mod no_missing_messages;

#[async_trait]
pub trait Validator: Send + Sync {
    fn name(&self) -> &'static str;
    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult>;
}

impl Named for dyn Validator {
    fn name(&self) -> &'static str {
        Validator::name(self)
    }
}

pub struct ValidatorEntry {
    pub depends_on: &'static [&'static str],
    pub validator: &'static (dyn Validator + Sync),
}

inventory::collect!(ValidatorEntry);

impl RegistryEntry for ValidatorEntry {
    type Item = dyn Validator;
    const KIND: &'static str = "validator";

    fn depends_on(&self) -> &'static [&'static str] {
        self.depends_on
    }
    fn value(&self) -> &'static dyn Validator {
        self.validator
    }
}

pub fn registry() -> Vec<&'static dyn Validator> {
    registry::topo_sort::<ValidatorEntry>(inventory::iter::<ValidatorEntry>)
}
