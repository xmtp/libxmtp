use xmtp_macro::Retryable;

#[derive(Retryable)]
enum E {
    #[retry(default = true)]
    Variant,
}

fn main() {}
