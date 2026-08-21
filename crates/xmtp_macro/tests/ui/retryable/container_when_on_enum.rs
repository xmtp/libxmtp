use xmtp_macro::Retryable;

// `when` on an enum container has no variant to bind; it is struct-only.
#[derive(Retryable)]
#[retry(when = 1 > 0)]
enum E {
    A,
}

fn main() {}
