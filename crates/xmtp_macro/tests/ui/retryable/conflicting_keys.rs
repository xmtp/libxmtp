use xmtp_macro::Retryable;

#[derive(Retryable)]
enum E {
    #[retry(true, false)]
    Both,
}

fn main() {}
