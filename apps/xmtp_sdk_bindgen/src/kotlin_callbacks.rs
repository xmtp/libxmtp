use anyhow::{Result, bail};

/// UniFFI's callback helpers catch Exception, which excludes Kotlin Error.
/// A Throwable must always report a call status so the Rust future completes.
pub fn rewrite(source: &str) -> Result<String> {
    const OLD: &str = "catch(e: kotlin.Exception)";
    const NEW: &str = "catch(e: Throwable)";
    let count = source.matches(OLD).count();
    if count == 0 {
        if source.contains(NEW) {
            return Ok(source.to_owned());
        }
        bail!("generated Kotlin callback helpers were not found");
    }
    if count != 4 {
        bail!("expected four Kotlin callback catches, found {count}");
    }
    Ok(source.replace(OLD, NEW))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_every_foreign_callback_throwable_and_is_idempotent() {
        let input = "catch(e: kotlin.Exception)".repeat(4);
        let output = rewrite(&input).unwrap();
        assert_eq!(output.matches("catch(e: Throwable)").count(), 4);
        assert_eq!(rewrite(&output).unwrap(), output);
        assert!(rewrite("no callback helpers").is_err());
    }
}
