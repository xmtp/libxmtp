use std::io::{self, Write};

fn main() -> io::Result<()> {
    write_greeting(io::stdout())
}

fn write_greeting(mut writer: impl Write) -> io::Result<()> {
    writeln!(writer, "Hello from the XMTP backend!")
}

#[cfg(test)]
mod tests {
    use super::write_greeting;

    #[xmtp_common::test(unwrap_try = true)]
    fn startup_message_is_written() {
        let mut output = Vec::new();
        write_greeting(&mut output)?;

        assert_eq!(output, b"Hello from the XMTP backend!\n");
    }
}
