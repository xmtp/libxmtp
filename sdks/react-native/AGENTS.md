# React Native SDK

```sh
dev/nix-shell 'just react-native check' # JS/types; no mobile build
dev/nix-shell 'just react-native test-react'
dev/nix-shell 'just react-native test-native'
dev/nix-shell 'just react-native test-control'
dev/nix-shell 'just react-native check-package'
```

- Ship matched prebuilt Rust libraries, generated C++, native adapters, and the
  committed fork runtime. Never substitute a same-version npm runtime.
- Keep codegen specs/config matched to Java/Objective-C++ providers and hashed
  in receipts. An explicit generated directory cannot bypass identity checks.
- `/react` imports the public root, creates no second runtime, and ships only
  with its compiled entry and types.
- Native consumers must install the same canonical tarball, enable Hermes/New
  Architecture, and never build Rust. JS/type or fixture checks do not prove this.
- Before packaging, cross-host exchange, or device CI changes, read
  [package development](DEVELOPMENT.md) and the
  [installed consumer guide](test/consumers/README.md).
