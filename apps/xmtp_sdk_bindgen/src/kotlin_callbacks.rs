use anyhow::{Result, bail};

/// UniFFI's callback helpers catch Exception, which excludes Kotlin Error.
/// A Throwable must always report a call status so the Rust future completes.
pub fn rewrite(source: &str) -> Result<String> {
    const OLD: &str = "catch(e: kotlin.Exception)";
    const NEW: &str = "catch(e: Throwable)";
    let count = source.matches(OLD).count();
    if count == 0 {
        if source.contains(NEW) {
            return wrap_foreign(source);
        }
        bail!("generated Kotlin callback helpers were not found");
    }
    if count != 4 {
        bail!("expected four Kotlin callback catches, found {count}");
    }
    wrap_foreign(&source.replace(OLD, NEW))
}

fn wrap_foreign(source: &str) -> Result<String> {
    let mut output = source.to_owned();
    for (name, wrapper) in [
        ("CredentialSource", "credentials"),
        ("LogSink", "logSink"),
        ("Signer", "signer"),
    ] {
        let anchor = format!("public object FfiConverterType{name}:");
        let Some(start) = output.find(&anchor) else {
            bail!("{name}: foreign converter was not found");
        };
        let end = output[start..]
            .find("\n}\n")
            .map(|length| start + length)
            .ok_or_else(|| anyhow::anyhow!("{name}: converter has no end"))?;
        let section = &output[start..end];
        let old = "return handleMap.insert(value)";
        let new = format!("return handleMap.insert(SDKForeign.{wrapper}(value))");
        if section.contains(old) {
            output.replace_range(start..end, &section.replace(old, &new));
        } else if !section.contains(&new) {
            bail!("{name}: foreign converter insert was not found");
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_every_foreign_callback_throwable_and_is_idempotent() {
        let helpers = [
            "uniffiTraitInterfaceCall",
            "uniffiTraitInterfaceCallWithError",
            "uniffiTraitInterfaceCallAsync",
            "uniffiTraitInterfaceCallAsyncWithError",
        ];
        let input = helpers
            .iter()
            .map(|name| format!("fun {name}() {{ catch(e: kotlin.Exception) }}"))
            .collect::<Vec<_>>()
            .join("\n")
            + &converter_fixture();
        let output = rewrite(&input).unwrap();
        for name in helpers {
            assert!(output.contains(&format!("fun {name}() {{ catch(e: Throwable) }}")));
        }
        assert_eq!(rewrite(&output).unwrap(), output);
        assert!(rewrite("no callback helpers").is_err());
    }

    #[test]
    fn every_foreign_converter_wraps_host_implementations() {
        let fixture = converter_fixture();
        let output = wrap_foreign(&fixture).unwrap();
        for wrapper in ["credentials", "logSink", "signer"] {
            assert!(output.contains(&format!("handleMap.insert(SDKForeign.{wrapper}(value))")));
        }
        assert_eq!(wrap_foreign(&output).unwrap(), output);
    }

    fn converter_fixture() -> String {
        ["CredentialSource", "LogSink", "Signer"]
            .iter()
            .map(|name| {
                format!(
                    "public object FfiConverterType{name}: Converter {{\n  return handleMap.insert(value)\n}}\n"
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
