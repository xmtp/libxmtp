use xmtp_macro::Retryable;

// `true`/`false`/`inherit` are variant-level keys; on an enum container only
// `default = <bool>` is meaningful.
#[derive(Retryable)]
#[retry(true)]
enum E {
    A,
}

fn main() {}
