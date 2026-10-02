//! Serialize foreign callback results before the async success branch.

use anyhow::{Context, Result, ensure};

const START: &str = "const uniffiMakeCall =";
const SUCCESS: &str = "const uniffiHandleSuccess = (returnValue: ";
const FOREIGN: &str = "const uniffiForeignFuture =";

pub(crate) fn rewrite(source: &str) -> Result<String> {
    let mut output = String::new();
    let mut rest = source;
    let mut results = Vec::new();
    while let Some(start) = rest.find(START) {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest
            .find(FOREIGN)
            .context("generated callback completion changed")?;
        let block = &rest[..end];
        let success = block
            .find(SUCCESS)
            .context("generated callback success changed")?;
        let ty_end = block[success + SUCCESS.len()..]
            .find(") => {")
            .map(|at| success + SUCCESS.len() + at)
            .context("generated callback result type changed")?;
        let ty = &block[success + SUCCESS.len()..ty_end];
        if ty == "void" {
            output.push_str(block);
        } else {
            let field = "return_value: ";
            let lower_start = block
                .find(field)
                .context("generated callback result field changed")?
                + field.len();
            let lower_end = block[lower_start..]
                .find(",\n")
                .map(|at| lower_start + at)
                .context("generated callback result lower changed")?;
            let lower = &block[lower_start..lower_end];
            let converter = lower
                .strip_suffix(".lower(returnValue, nativeModule().rustbuffer_alloc)")
                .context("generated callback result converter changed")?;
            let make = &block[..success];
            let promise = format!(": Promise<{ty}>");
            ensure!(
                make.matches(&promise).count() == 1,
                "generated callback promise type changed"
            );
            let call_start = make
                .find("return await ")
                .context("generated callback async call changed")?;
            let call_end = make
                .rfind("};")
                .context("generated callback async end changed")?;
            let call = make[call_start + "return await ".len()..call_end]
                .trim()
                .trim_end_matches(';');
            let mut patched = make[..call_start].replace(&promise, ": Promise<UniffiByteArray>");
            patched.push_str(&format!("return {converter}.lower(await {call}, nativeModule().rustbuffer_alloc);\n            }};\n            "));
            patched.push_str(
                &block[success..]
                    .replacen(
                        &format!("{SUCCESS}{ty}"),
                        &format!("{SUCCESS}UniffiByteArray"),
                        1,
                    )
                    .replacen(lower, "returnValue", 1),
            );
            output.push_str(&patched);
            results.push(ty.to_owned());
        }
        rest = &rest[end..];
    }
    results.sort();
    ensure!(
        results == ["Credential", "PublicIdentity", "Signature", "SignerKind"],
        "generated callback result set changed: {results:?}"
    );
    output.push_str(rest);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callback(ty: &str) -> String {
        format!(
            r#"const uniffiMakeCall =
            async (signal: AbortSignal)
            : Promise<{ty}> => {{
                const jsCallback = FfiConverterTypeSource.lift(uniffiHandle);
                return await jsCallback.callback({{ signal }}
                )
            }};
            const uniffiHandleSuccess = (returnValue: {ty}) => {{
                uniffiFutureCallback.call(uniffiFutureCallback, uniffiCallbackData, {{
                        return_value: FfiConverterType{ty}.lower(returnValue, nativeModule().rustbuffer_alloc),
                        call_status: uniffiCaller.createCallStatus()
                }});
            }};
            const uniffiHandleError = (code: number, errorBuf: UniffiByteArray) => {{
                uniffiFutureCallback.call(uniffiFutureCallback, uniffiCallbackData, {{
                    return_value: new Uint8Array(0),
                    call_status: uniffiCaller.createErrorStatus(code, errorBuf),
                }});
            }};
            const uniffiForeignFuture = uniffiTraitInterfaceCallAsyncWithError(
                uniffiMakeCall, uniffiHandleSuccess, uniffiHandleError);
"#
        )
    }

    fn fixture() -> String {
        ["Credential", "PublicIdentity", "SignerKind", "Signature"]
            .map(callback)
            .join("\n")
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn callback_result_serialization_precedes_success_for_credentials_and_signers() {
        let void = callback("void");
        let output = rewrite(&(fixture() + &void))?;
        assert!(output.contains(&void));
        assert_eq!(output.matches(": Promise<UniffiByteArray>").count(), 4);
        assert_eq!(output.matches("return_value: returnValue").count(), 4);
        for ty in ["Credential", "PublicIdentity", "SignerKind", "Signature"] {
            assert!(output.contains(&format!(
                "return FfiConverterType{ty}.lower(await jsCallback.callback"
            )));
            assert!(!output.contains(&format!("FfiConverterType{ty}.lower(returnValue")));
        }
        assert_eq!(
            output
                .matches("uniffiTraitInterfaceCallAsyncWithError(")
                .count(),
            5
        );
        assert_eq!(
            output
                .matches("uniffiCaller.createErrorStatus(code, errorBuf)")
                .count(),
            5
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn callback_result_serialization_rejects_changed_result_or_callback_set() {
        let source = fixture();
        assert!(
            rewrite(&source.replace(
                "return_value: FfiConverterTypeCredential.lower",
                "return_value: other"
            ))
            .is_err()
        );
        assert!(rewrite(&source.replace(": Promise<Credential>", ": Other<Credential>")).is_err());
        assert!(rewrite(&callback("Credential")).is_err());
        assert!(rewrite(&(source + &callback("Credential"))).is_err());
    }
}
