use xmtp_macro::Retryable;

// `inherit` forwards a variant's single field; it is meaningless on a struct
// container (use `when = self.field.is_retryable()` instead).
#[derive(Retryable)]
#[retry(inherit)]
struct S {
    inner: u32,
}

fn main() {}
