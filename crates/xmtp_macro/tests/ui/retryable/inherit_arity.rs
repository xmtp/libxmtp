use xmtp_macro::Retryable;

#[derive(Retryable)]
enum E {
    // inherit needs exactly one field; this has two
    #[retry(inherit)]
    TwoFields(u32, u32),
}

fn main() {}
