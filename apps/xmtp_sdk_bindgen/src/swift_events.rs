use anyhow::{Result, bail};

const READER: &str = "open class EventReader: EventReaderProtocol, @unchecked Sendable {";
const STORAGE: &str = "open class Storage: StorageProtocol, @unchecked Sendable {";
const CLIENT: &str = "open class Client: ClientProtocol, @unchecked Sendable {";
const STATE: &str = r#"internal final class SdkEventReadGate: @unchecked Sendable {
    private let lock = NSLock()
    private var ended = false

    func stop() { lock.lock(); ended = true; lock.unlock() }
    func isEnded() -> Bool { lock.lock(); defer { lock.unlock() }; return ended }
    func handoff<T>(_ value: T?) -> (ended: Bool, value: T?) {
        lock.lock()
        defer { lock.unlock() }
        return (ended, ended ? nil : value)
    }
}

fileprivate final class SdkWeakEventGate {
    weak var value: SdkEventReadGate?
    init(_ value: SdkEventReadGate) { self.value = value }
}

internal final class SdkEventReadGates: @unchecked Sendable {
    private let lock = NSLock()
    private var ended = false
    private var readers: [SdkWeakEventGate] = []

    func add(_ gate: SdkEventReadGate) {
        lock.lock()
        defer { lock.unlock() }
        if ended { gate.stop(); return }
        readers.removeAll { $0.value == nil }
        readers.append(SdkWeakEventGate(gate))
    }

    func stopAll() {
        lock.lock()
        defer { lock.unlock() }
        ended = true
        for reader in readers { reader.value?.stop() }
        readers.removeAll()
    }
}

"#;

fn method(
    source: &mut String,
    class: &str,
    name: &str,
    change: impl FnOnce(&str) -> Result<String>,
) -> Result<()> {
    let start = source
        .find(class)
        .ok_or_else(|| anyhow::anyhow!("Swift {class} changed"))?;
    let end = source[start + class.len()..]
        .find("\nopen class ")
        .map_or(source.len(), |end| start + class.len() + end);
    let anchor = format!("\nopen func {name}(");
    let begin = source[start..end]
        .find(&anchor)
        .ok_or_else(|| anyhow::anyhow!("Swift {name} method changed"))?
        + start;
    let finish = source[begin..end]
        .find("\n}\n")
        .ok_or_else(|| anyhow::anyhow!("Swift {name} method has no end"))?
        + begin
        + 3;
    let replacement = change(&source[begin..finish])?;
    source.replace_range(begin..finish, &replacement);
    Ok(())
}

fn once(source: &str, old: &str, new: &str) -> Result<String> {
    if source.matches(old).count() != 1 {
        bail!("pinned Swift event boundary changed: {old}");
    }
    Ok(source.replacen(old, new, 1))
}

fn open_method(body: &str, opening: &str) -> Result<String> {
    let (declaration, rest) = body
        .split_once(" {\n")
        .ok_or_else(|| anyhow::anyhow!("pinned Swift method opening changed"))?;
    if declaration.contains('{') {
        bail!("pinned Swift method declaration changed");
    }
    Ok(format!("{declaration}{opening}{rest}"))
}

pub fn rewrite(source: &str) -> Result<String> {
    if !source.contains(READER) {
        return Ok(source.to_owned());
    }
    let mut output = once(
        source,
        READER,
        &format!("{READER}\n    internal let sdkEventReadGate = SdkEventReadGate()"),
    )?;
    output = once(
        &output,
        CLIENT,
        &format!("{CLIENT}\n    internal let sdkEventReadGates = SdkEventReadGates()"),
    )?;
    output = once(
        &output,
        STORAGE,
        &format!("{STORAGE}\n    internal var sdkEventReadGates: SdkEventReadGates?"),
    )?;
    method(&mut output, READER, "end", |body| {
        let body = open_method(
            body,
            " {\n    sdkEventReadGate.stop()\n    return try await Task.detached { [self] in\n",
        )?;
        once(&body, "\n}\n", "\n    }.value\n}\n")
    })?;
    method(&mut output, READER, "next", |body| {
        let body = open_method(
            body,
            " {\n    if sdkEventReadGate.isEnded() { return nil }\n",
        )?;
        once(
            &body,
            "liftFunc: FfiConverterOptionTypeClientEvent.lift,",
            "eventReadResult: { let result = self.sdkEventReadGate.handoff($0); if result.ended { try await self.end() }; return result.value },\n            endCancelledEventRead: { try await self.end(); return nil },\n            cancelEventRead: { self.sdkEventReadGate.stop() },\n            liftFunc: FfiConverterOptionTypeClientEvent.lift,",
        )
    })?;
    method(&mut output, CLIENT, "end", |body| {
        let body = open_method(
            body,
            " {\n    sdkEventReadGates.stopAll()\n    try await Task.detached { [self] in\n",
        )?;
        let body = once(&body, "    return\n", "    _ =\n")?;
        once(
            &body,
            "\n}\n",
            "\n    }.value\n    try Task.checkCancellation()\n}\n",
        )
    })?;
    method(&mut output, CLIENT, "events", |body| {
        let body = once(body, "    return\n", "    let reader =\n")?;
        once(
            &body,
            "\n}\n",
            "\n    sdkEventReadGates.add(reader.sdkEventReadGate)\n    return reader\n}\n",
        )
    })?;
    method(&mut output, CLIENT, "storage", |body| {
        let body = once(body, "    return ", "    let storage = ")?;
        once(
            &body,
            "\n}\n",
            "\n    storage.sdkEventReadGates = sdkEventReadGates\n    return storage\n}\n",
        )
    })?;
    method(&mut output, STORAGE, "delete", |body| {
        let body = open_method(
            body,
            " {\n    let fileBacked = try await path() != nil\n    try Task.checkCancellation()\n    if fileBacked { sdkEventReadGates?.stopAll() }\n    try await Task.detached { [self] in\n",
        )?;
        let body = once(&body, "    return\n", "    _ =\n")?;
        once(
            &body,
            "\n}\n",
            "\n    }.value\n    try Task.checkCancellation()\n}\n",
        )
    })?;
    output = format!("{STATE}{output}");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINNED: &str = include_str!("swift_event_fixture.swift");

    #[xmtp_common::test(unwrap_try = true)]
    fn rewrites_pinned_event_boundaries_with_nested_closures() {
        let output = rewrite(PINNED)?;
        assert!(output.contains("sdkEventReadGate.stop()"));
        assert!(output.contains("return try await Task.detached { [self] in"));
        assert!(output.contains("eventReadResult: { let result = self.sdkEventReadGate.handoff($0); if result.ended { try await self.end() }; return result.value }"));
        assert!(output.contains("endCancelledEventRead: { try await self.end(); return nil }"));
        assert!(output.contains("sdkEventReadGates.stopAll()"));
        assert!(output.contains("sdkEventReadGates.add(reader.sdkEventReadGate)"));
        assert!(output.contains("storage.sdkEventReadGates = sdkEventReadGates"));
        assert!(output.contains("if fileBacked { sdkEventReadGates?.stopAll() }"));
        let message = PINNED.split("open class MessageReader:").nth(1).unwrap();
        assert_eq!(
            output.split("open class MessageReader:").nth(1).unwrap(),
            message
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_changed_event_binding_and_preserves_pure_output() {
        assert!(
            rewrite(&PINNED.replace(
                "liftFunc: FfiConverterOptionTypeClientEvent.lift,",
                "liftFunc: changed,"
            ))
            .is_err()
        );
        assert_eq!(rewrite("pure bindings")?, "pure bindings");
    }
}
