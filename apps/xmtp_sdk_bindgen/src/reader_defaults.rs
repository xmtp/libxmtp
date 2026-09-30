use anyhow::{Result, bail};

/// Preserve default reader options in the interfaces returned by SDK objects.
pub(crate) fn rewrite(source: &str) -> Result<String> {
    let mut output = source.to_owned();
    for (options, count) in [
        ("MessageReaderOptions", 1),
        ("ConversationMessageReaderOptions", 2),
    ] {
        let required = format!("messageReader(options: {options} | undefined, asyncOpts_?");
        if output.matches(&required).count() != count {
            bail!("generated TypeScript reader interface changed: {options}");
        }
        output = output.replace(
            &required,
            &format!("messageReader(options?: {options} | undefined, asyncOpts_?"),
        );
    }
    Ok(output)
}
