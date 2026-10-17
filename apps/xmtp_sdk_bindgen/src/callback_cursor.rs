use anyhow::{Result, bail};

const CONVERTER: &str = "new FfiConverterObjectWithCallbacks(";
const CALLER: &str = "const uniffiCaller = new UniffiRustCaller(() => ({ code: 0 }));";
const PATCH: &str = r#"

// The pinned player inherits writeIntoCursor from the Rust-only object
// converter. Optional foreign objects in records must use the callback map.
class FfiConverterCallbackObjectWithCursor<T> extends FfiConverterObjectWithCallbacks<T> {
  override writeIntoCursor(value: T, cursor: Cursor): void {
    FfiConverterUInt64.writeIntoCursor(this.lower(value, nativeModule().rustbuffer_alloc), cursor);
  }
}
"#;

pub(crate) fn rewrite(source: &str) -> Result<String> {
    if !source.contains(CONVERTER) || !source.contains(CALLER) {
        bail!("generated TypeScript callback converter shape changed");
    }
    Ok(source
        .replacen(CALLER, &format!("{CALLER}{PATCH}"), 1)
        .replace(CONVERTER, "new FfiConverterCallbackObjectWithCursor("))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn optional_foreign_object_uses_callback_cursor() {
        let source = format!("{CALLER}\nconst converter = {CONVERTER}factory);");
        let rewritten = rewrite(&source)?;
        assert!(rewritten.contains("this.lower(value, nativeModule().rustbuffer_alloc)"));
        assert!(rewritten.contains("new FfiConverterCallbackObjectWithCursor(factory)"));
        assert!(rewrite("different template").is_err());
    }
}
