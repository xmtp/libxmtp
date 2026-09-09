//! Query Combinators
//!
//! Combinators extend the functionality of endpoints. They are specific to endpoints
//! and are not concerned with the underlying `Client` implementation

mod retry;
pub use retry::*;

mod ignore;
pub use ignore::*;
