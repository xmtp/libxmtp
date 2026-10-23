# iOS example

```sh
dev/nix-shell 'just example-ios build'
dev/nix-shell 'just example-ios test'
dev/nix-shell 'just example-ios test-ui' # worktree backend required
dev/nix-shell 'just example-ios lint'
```

- Only ExampleSDK and the UI-test runner import XmtpSdk. ExampleCore imports
  Foundation, Observation, CryptoKit only. Route stores own private-set state
  and actions; views consume them.
- Admit async results by both SessionKey and destination token; recreate stores
  when the token changes. Cancel/await cleanup before ending the client; no
  `Task.detached`.
- Background push restores only existing profiles, starts no UI collectors,
  and serializes registration with shutdown. Preserve privacy admission and the
  25-second deadline that revokes late results and completes once.
- Use system/design components and feature string catalogs. Add light, dark,
  AX3 previews; verify simulator pixels, AX5, VoiceOver, and iPad. Source/previews
  and APNs simulator builds are not device proofs.
- Preserve journey coverage of handle claim, request acceptance, and reader
  recovery after background kill. Record test mutations and manual results.
- Build, push setup, design, and manual checks: [README](README.md).
