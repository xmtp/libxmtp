//! Validators run after ops + the final sync. They check that all clients
//! converged (no forks, no missing messages).

use crate::app::health::context::HealthContext;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

#[async_trait]
pub trait Validator: Send + Sync {
    fn name(&self) -> &'static str;
    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult>;
}

pub fn registry() -> Vec<Box<dyn Validator>> {
    Vec::new()
}
