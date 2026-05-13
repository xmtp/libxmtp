//! Op: record that the run's primary identity was successfully created.
//! The actual creation happens during `HealthContext::bootstrap`; this op
//! exposes that success as a discrete check in the run's result table.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;

pub struct CreateIdentity;

#[async_trait]
impl HealthOp for CreateIdentity {
    fn name(&self) -> &'static str {
        "CreateIdentity"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let start = Instant::now();
        let inbox_id = ctx.primary.inbox_id().to_string();
        let status = if inbox_id.is_empty() {
            Status::Fail
        } else {
            Status::Pass
        };
        vec![OpResult {
            op_name: self.name(),
            target: Some(inbox_id),
            status,
            duration: start.elapsed(),
            error: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(CreateIdentity.name(), "CreateIdentity");
    }
}
