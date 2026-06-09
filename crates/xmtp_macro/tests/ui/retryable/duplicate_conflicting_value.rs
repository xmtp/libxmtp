use xmtp_macro::Retryable;

// Without duplicate detection the last attribute would silently win and flip
// every is_retryable() result.
#[derive(Retryable)]
#[retry(default = true)]
#[retry(default = false)]
enum E {
    A,
}

fn main() {}
