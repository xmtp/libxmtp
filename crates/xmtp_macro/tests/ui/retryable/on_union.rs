use xmtp_macro::Retryable;

#[derive(Retryable)]
union U {
    a: u32,
    b: f32,
}

fn main() {}
