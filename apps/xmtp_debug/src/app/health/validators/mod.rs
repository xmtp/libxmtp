//! Validators run after ops + the final sync. They check that all clients
//! converged (no forks, no missing messages).
//!
//! Each validator self-registers via `inventory::submit!` and is sorted by
//! declared `depends_on` relationships, same shape as `ops::OpEntry`.

use crate::app::health::context::HealthContext;
use crate::app::health::registry::{self, RegistryEntry};
use crate::app::health::result::OpResult;
use async_trait::async_trait;

mod no_forks;
mod no_missing_messages;

#[async_trait]
pub trait Validator: Send + Sync {
    fn name(&self) -> &'static str;
    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult>;
}

pub struct ValidatorEntry {
    pub name: &'static str,
    pub depends_on: &'static [&'static str],
    pub make: fn() -> Box<dyn Validator>,
}

inventory::collect!(ValidatorEntry);

impl RegistryEntry for ValidatorEntry {
    type Item = dyn Validator;
    const KIND: &'static str = "validator";

    fn name(&self) -> &'static str {
        self.name
    }
    fn depends_on(&self) -> &'static [&'static str] {
        self.depends_on
    }
    fn make(&self) -> Box<dyn Validator> {
        (self.make)()
    }
}

pub fn registry() -> Vec<Box<dyn Validator>> {
    registry::topo_sort::<ValidatorEntry>(inventory::iter::<ValidatorEntry>)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_name_matches_validator_name() {
        for entry in inventory::iter::<ValidatorEntry> {
            let v = (entry.make)();
            assert_eq!(
                entry.name,
                v.name(),
                "ValidatorEntry::name disagrees with Validator::name() for one validator",
            );
        }
    }
}
