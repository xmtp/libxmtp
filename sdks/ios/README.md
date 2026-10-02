# XMTP iOS SDK

The iOS package exports `XmtpSdk`. Rust owns messaging, storage, content, and
attachment transfers. Swift provides app storage paths, streams, callbacks, and
Apple system logging.

Add this repository as a Swift package in Xcode. Select the `XmtpSdk` product.
Use `import XmtpSdk`. The package has one static native XCFramework. CocoaPods
uses the same archive and SHA256 receipt. The pod name is `XMTP`; its module is
`XmtpSdk`. This change starts at version `8.0.0`.

For a local CocoaPods checkout, first run `dev/nix-shell 'just ios build'`.
Point the Podfile at `sdks/ios` with `:path`. When no release receipt exists,
the podspec selects `Artifacts/XmtpSdkFFI.xcframework`. A released pod uses the
archive URL and checksum in `ReleaseArtifacts.json`. An invalid receipt fails
evaluation. The `ios-<version>` tag is a release source template. It does not
mean that an unpublished version has an available tag or archive.

Create a client with a signer and explicit backend options:

```swift
import XmtpSdk

let signer = await generateLocalSigner()
let client = try await SDKClient.create(
    signer: signer,
    options: ClientOptions(
        backend: .options(options: BackendOptions(url: "https://backend.example")),
        storage: StorageOptions(location: .default)
    )
)
let group = try await client.conversations().createGroup(members: [String](), options: nil)
_ = try await group.sendText(text: "Hello", options: nil)
for try await message in try await client.messages(in: group) {
    if case .standard(.text(let text)) = message.content { print(text) }
}
try await client.end()
```

Default storage uses the app's Application Support directory and bundle ID.
Use `.directory` or `.explicit` when the app selects its own path. Use
`.inMemory` for a temporary client. Call `end()` when the app releases a client.
Automatic Apple lifecycle management is on by default. It suspends live streams
when the app enters the background and resumes them when the app becomes active.

Standard codecs call Rust. Custom codecs belong to one client. Pass them to
`SDKClient.create` or `SDKClient.build`. Message content is typed; unknown
content keeps its bytes, fallback, and error details.

Run commands from the repository root:

```sh
dev/nix-shell 'just ios build'
dev/nix-shell 'just ios check'
dev/nix-shell 'just ios check-examples'
dev/nix-shell 'just ios test'
dev/nix-shell 'just ios test-simulator'
```

Both [example apps](./example) and [XMTPiOSExample](./XMTPiOSExample) use the
local package. Set `XMTP_BACKEND_URL` to the backend for the tested commit. A
physical device needs a backend address that it can reach.
