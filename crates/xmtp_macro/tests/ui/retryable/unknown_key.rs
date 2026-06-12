use xmtp_macro::Retryable;

#[derive(Retryable)]
enum E {
    #[retry(sometimes)]
    Bogus,
}

fn main() {}
