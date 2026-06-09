use xmtp_macro::Retryable;

// The same key twice (even with equal values) is rejected — a duplicate is
// always a copy-paste or merge artifact.
#[derive(Retryable)]
enum E {
    #[retry(inherit)]
    #[retry(inherit)]
    Wrapped(u32),
}

fn main() {}
