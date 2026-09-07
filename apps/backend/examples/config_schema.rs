fn main() -> Result<(), serde_json::Error> {
    println!(
        "{}",
        serde_json::to_string_pretty(&xmtp_backend::config::Config::schema())?
    );
    Ok(())
}
