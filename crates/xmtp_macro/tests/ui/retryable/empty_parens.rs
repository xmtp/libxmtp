use xmtp_macro::Retryable;

// `#[retry()]` is malformed — without rejection it would silently fall back to
// the baseline instead of acting like bare `#[retry]`.
#[derive(Retryable)]
enum E {
    #[retry()]
    A,
}

fn main() {}
