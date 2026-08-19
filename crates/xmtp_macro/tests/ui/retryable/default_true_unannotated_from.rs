use thiserror::Error;
use xmtp_macro::Retryable;

// Under a retryable-by-default enum, an unannotated `#[from]` variant would
// silently retry a permanent foreign error forever; the policy must be stated.
#[derive(Debug, Error, Retryable)]
#[retry(default = true)]
enum E {
    #[error(transparent)]
    Wrapped(#[from] core::num::ParseIntError),
}

fn main() {}
