use anyhow::{Result, bail};

const ENCODE_STANDARD: &str =
    "export function encodeStandard(value: StandardContent): EncodedContent /*throws*/ {";

pub(crate) fn rewrite(source: &str) -> Result<String> {
    if source.matches(ENCODE_STANDARD).count() != 1 {
        bail!("generated TypeScript must have one encodeStandard export");
    }
    let raw = source.replacen(
        ENCODE_STANDARD,
        "function encodeStandardRaw(value: StandardContent): EncodedContent /*throws*/ {",
        1,
    );
    Ok(format!(
        "import {{ validatedIdLiftMessage }} from './runtime/validated-id-lift';\n{raw}\n\
         export function encodeStandard(value: StandardContent): EncodedContent {{\n\
           try {{\n\
             return encodeStandardRaw(value);\n\
           }} catch (error) {{\n\
             const message = validatedIdLiftMessage(error);\n\
             if (message !== undefined) {{\n\
               throw new XmtpError.InvalidArgument({{\n\
                 code: 'InvalidArgument',\n\
                 category: ErrorCategory.Input,\n\
                 retryable: false,\n\
                 message,\n\
               }});\n\
             }}\n\
             throw error;\n\
           }}\n\
         }}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fails_closed_if_generator_moves_the_export() {
        assert!(rewrite("export function somethingElse() {}").is_err());
        assert!(rewrite(&format!("{ENCODE_STANDARD}\n{ENCODE_STANDARD}")).is_err());
        let output = rewrite(ENCODE_STANDARD).unwrap();
        assert!(output.contains("function encodeStandardRaw("));
        assert!(output.contains("export function encodeStandard("));
    }
}
