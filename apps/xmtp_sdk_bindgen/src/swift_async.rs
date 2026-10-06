use anyhow::{Result, bail};

const HELPER: &str = "fileprivate func uniffiRustCallAsync<F, T>(";
const CANCELLED: &str = "            fatalError(\"Cancellation not supported yet\")";
const SIGNATURE: &str = "    freeFunc: (UInt64) -> (),";
const OLD_BODY: &str = r#"    defer {
        freeFunc(rustFuture)
    }
    var pollResult: Int8;
    repeat {
        pollResult = await withUnsafeContinuation {
            pollFunc(
                rustFuture,
                { handle, pollResult in
                    uniffiFutureContinuationCallback(handle: handle, pollResult: pollResult)
                },
                uniffiContinuationHandleMap.insert(obj: $0)
            )
        }
    } while pollResult != UNIFFI_RUST_FUTURE_POLL_READY

    return try liftFunc(makeRustCall(
        { completeFunc(rustFuture, $0) },
        errorHandler: errorHandler
    ))"#;
const NEW_BODY: &str = r#"    let allowsCancellation = errorHandler != nil
    let future = UniffiCancellableRustFuture(rustFuture, cancel: cancelFunc, free: freeFunc)
    defer { future.free() }
    return try await withTaskCancellationHandler(operation: {
        do {
        if allowsCancellation { try Task.checkCancellation() }
        var pollResult: Int8
        repeat {
            pollResult = await withUnsafeContinuation {
                pollFunc(
                    rustFuture,
                    { handle, pollResult in
                        uniffiFutureContinuationCallback(handle: handle, pollResult: pollResult)
                    },
                    uniffiContinuationHandleMap.insert(obj: $0)
                )
            }
        } while pollResult != UNIFFI_RUST_FUTURE_POLL_READY
        // Lift stored ready results before the caller applies its handoff policy.
        let value = try future.complete { handle in
            try makeRustCall({ completeFunc(handle, $0) }, errorHandler: errorHandler)
        }
        let lifted = try liftFunc(value)
        if let discard = discardReadyOnCancellation, Task.isCancelled {
            let cancellation = CancellationError()
            do {
                try await Task.detached { try await discard(lifted) }.value
            } catch {
                NSLog("Discarded Client cleanup failed")
            }
            throw cancellation
        }
        if let endCancelledEventRead, Task.isCancelled {
            cancelEventRead?()
            return try await Task.detached { try await endCancelledEventRead() }.value
        }
        if let eventReadResult { return try await eventReadResult(lifted) }
        return lifted
        } catch let cancellation as CancellationError {
            guard let endCancelledEventRead else { throw cancellation }
            cancelEventRead?()
            return try await Task.detached { try await endCancelledEventRead() }.value
        }
    }, onCancel: {
        cancelEventRead?()
        if allowsCancellation { future.cancel() }
    })"#;

// The native functions can run on any thread. The lock protects their shared handle.
const STATE: &str = r#"fileprivate final class UniffiCancellableRustFuture: @unchecked Sendable {
    private let lock = NSLock()
    private let handle: UInt64
    private let cancelFunc: (UInt64) -> Void
    private let freeFunc: (UInt64) -> Void
    private var cancelled = false
    private var completed = false
    private var freed = false

    init(_ handle: UInt64, cancel: @escaping (UInt64) -> Void, free: @escaping (UInt64) -> Void) {
        self.handle = handle
        cancelFunc = cancel
        freeFunc = free
    }

    func cancel() {
        lock.lock()
        defer { lock.unlock() }
        guard !freed && !cancelled && !completed else { return }
        cancelled = true
        cancelFunc(handle)
    }

    func complete<T>(_ body: (UInt64) throws -> T) throws -> T {
        lock.lock()
        defer { lock.unlock() }
        precondition(!freed && !completed)
        completed = true
        return try body(handle)
    }

    func free() {
        lock.lock()
        defer { lock.unlock() }
        guard !freed else { return }
        freed = true
        freeFunc(handle)
    }
}

"#;

/// Add caller cancellation without throwing from nonthrowing bindings.
pub fn rewrite(source: &str) -> Result<String> {
    if !source.contains(HELPER) {
        if source.contains("freeFunc: ffi_") {
            bail!("Swift async calls have no pinned caller helper");
        }
        return Ok(source.to_owned());
    }
    for (name, anchor) in [
        ("caller helper", HELPER),
        ("free argument", SIGNATURE),
        ("caller body", OLD_BODY),
        ("cancelled status", CANCELLED),
    ] {
        let count = source.matches(anchor).count();
        if count != 1 {
            bail!("expected one pinned Swift {name}, found {count}");
        }
    }
    let mut output = String::new();
    let mut calls = 0;
    let mut caller = None;
    let mut discard_ready = false;
    for line in source.lines() {
        if let Some((function, _)) = line.trim().split_once('(')
            && function.starts_with("uniffi_xmtp_sdk_fn_")
        {
            caller = Some(function);
            discard_ready = matches!(
                function,
                "uniffi_xmtp_sdk_fn_constructor_client_build"
                    | "uniffi_xmtp_sdk_fn_constructor_client_create"
                    | "uniffi_xmtp_sdk_fn_method_sdkconformanceconstructorprobe_build_ready"
                    | "uniffi_xmtp_sdk_fn_method_sdkconformanceconstructorprobe_create_ready"
            );
        }
        if let Some(lift) = line.trim().strip_prefix("liftFunc: ") {
            let owns_client = lift == "FfiConverterTypeClient_lift,";
            if owns_client != discard_ready {
                bail!("Swift Client future has no cancellation policy: {caller:?}");
            }
            if discard_ready {
                let indent = &line[..line.len() - line.trim_start().len()];
                output.push_str(&format!("{indent}discardReadyOnCancellation: {{ try await sdkDiscardUnreturnedClient(client: $0) }},\n"));
            }
            caller = None;
            discard_ready = false;
        }
        if let Some(function) = line
            .trim()
            .strip_prefix("freeFunc: ")
            .filter(|value| value.starts_with("ffi_"))
        {
            let function = function
                .strip_suffix(',')
                .ok_or_else(|| anyhow::anyhow!("Swift free call has no comma"))?;
            let suffix = function
                .strip_prefix("ffi_xmtp_sdk_rust_future_free_")
                .ok_or_else(|| anyhow::anyhow!("unknown Swift future free function: {function}"))?;
            if suffix.is_empty()
                || !suffix
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                bail!("invalid Swift future free suffix: {suffix}");
            }
            let indent = &line[..line.len() - line.trim_start().len()];
            output.push_str(&format!(
                "{indent}cancelFunc: ffi_xmtp_sdk_rust_future_cancel_{suffix},\n"
            ));
            calls += 1;
        }
        output.push_str(line);
        output.push('\n');
    }
    if calls == 0
        || calls != source.matches("freeFunc: ffi_").count()
        || calls != source.matches("await uniffiRustCallAsync(").count()
    {
        bail!("Swift async call-site count does not match future free functions");
    }
    output = output.replace(
        SIGNATURE,
        "    cancelFunc: @escaping (UInt64) -> (),\n    freeFunc: @escaping (UInt64) -> (),\n    discardReadyOnCancellation: ((T) async throws -> Void)? = nil,\n    eventReadResult: ((T) async throws -> T)? = nil,\n    endCancelledEventRead: (() async throws -> T)? = nil,\n    cancelEventRead: (() -> Void)? = nil,",
    );
    output = output.replace(OLD_BODY, NEW_BODY);
    output = output.replace(CANCELLED, "            throw CancellationError()");
    output = output.replace(HELPER, &format!("{STATE}{HELPER}"));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        format!(
            "{HELPER}\n{SIGNATURE}\n{OLD_BODY}\n{CANCELLED}\ntry await uniffiRustCallAsync(\nfreeFunc: ffi_xmtp_sdk_rust_future_free_rust_buffer,\ntry await uniffiRustCallAsync(\nfreeFunc: ffi_xmtp_sdk_rust_future_free_u64,\n"
        )
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn swift_cancellation_matches_every_call_site_and_rejects_template_drift() {
        let source = fixture();
        let output = rewrite(&source).unwrap();
        assert!(output.contains("cancelFunc: ffi_xmtp_sdk_rust_future_cancel_rust_buffer,"));
        assert!(output.contains("cancelFunc: ffi_xmtp_sdk_rust_future_cancel_u64,"));
        assert_eq!(output.matches("withTaskCancellationHandler").count(), 1);
        assert!(!output.contains("Cancellation not supported yet"));
        for anchor in [HELPER, SIGNATURE, OLD_BODY, CANCELLED] {
            assert!(rewrite(&source.replace(anchor, "changed")).is_err());
            assert!(rewrite(&format!("{source}{anchor}")).is_err());
        }
        assert!(rewrite(&source.replace("free_rust_buffer", "unknown_rust_buffer")).is_err());
        assert_eq!(rewrite("pure bindings\n").unwrap(), "pure bindings\n");
        assert!(output.contains("let allowsCancellation = errorHandler != nil"));
        assert!(output.contains("if allowsCancellation { future.cancel() }"));
        assert!(output.contains("try await Task.detached { try await discard(lifted) }.value"));
        // The Swift cancellation tests cancel a call after its task first
        // suspends. That is past the cancellation check only while no `await`
        // comes before it.
        let operation = output
            .find("withTaskCancellationHandler(operation: {")
            .expect("cancellation handler");
        let check = output
            .find("try Task.checkCancellation()")
            .expect("cancellation check");
        assert!(operation < check && !output[operation..check].contains("await"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn swift_client_ready_discard_is_private_and_requires_named_callers() {
        let caller = "uniffi_xmtp_sdk_fn_constructor_client_create";
        let source = fixture().replace("freeFunc: ffi_xmtp_sdk_rust_future_free_u64,",
            &format!("{caller}(FfiConverterTypeSigner_lower(signer),\nfreeFunc: ffi_xmtp_sdk_rust_future_free_u64,\nliftFunc: FfiConverterTypeClient_lift,"));
        let output = rewrite(&source).unwrap();
        assert_eq!(
            output.matches("discardReadyOnCancellation: { try await sdkDiscardUnreturnedClient(client: $0) },").count(),
            1
        );
        assert!(
            rewrite(&source.replace(caller, "uniffi_xmtp_sdk_fn_method_other_client")).is_err()
        );
        assert!(
            rewrite(&source.replace(
                "FfiConverterTypeClient_lift,",
                "FfiConverterTypeOther_lift,"
            ))
            .is_err()
        );
        for probe in ["build_ready", "create_ready"] {
            let name = format!("uniffi_xmtp_sdk_fn_method_sdkconformanceconstructorprobe_{probe}");
            assert_eq!(
                rewrite(&source.replace(caller, &name))
                    .unwrap()
                    .matches("discardReadyOnCancellation: { try await sdkDiscardUnreturnedClient(client: $0) },")
                    .count(),
                1
            );
        }
    }
}
