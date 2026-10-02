# Contributing

Thank you for considering contributing to this repo. Community contributions like yours are key to the development and adoption of XMTP. Your questions, feedback, suggestions, and code contributions are welcome.

## ❔ Questions

Have a question about how to build with XMTP? Ask your question and learn with the community in the [Q&A discussion forum](https://github.com/orgs/xmtp/discussions/categories/q-a).

## 🐞 Bugs

Report bugs as [GitHub Issues](https://github.com/xmtp/libxmtp/issues/new).
Check for an existing report. Include the steps that cause the bug.

## ✨ Feature requests

Submit feature requests as [GitHub Issues](https://github.com/xmtp/libxmtp/issues/new).
Check for an existing request. Describe the required behavior and its caller.

## 🔀 Pull requests

Describe the required change before you start a large PR. For protocol changes,
read the approved [specifications](../../docs/specs/) first.

### AI-Generated Contributions Policy

We do not accept pull requests that are generated entirely or primarily by AI/LLM tools (e.g., GitHub Copilot, ChatGPT, Claude). This includes:

- Automated typo fixes or formatting changes
- Generic code improvements without context
- Mass automated updates or refactoring

Pull requests that appear to be AI-generated without meaningful human oversight will be closed without review. We value human-driven, thoughtful contributions that demonstrate an understanding of the codebase and project goals.

> [!CAUTION]
> To protect project quality and maintain contributor trust, we will restrict access for users who continue to submit AI-generated pull requests.

If you use AI tools to assist your development process, please:

1. Thoroughly review and understand all generated code
2. Provide detailed PR descriptions explaining your changes and reasoning
3. Be prepared to discuss your implementation decisions and how they align with the project goals

## 🔧 Developing

### Prerequisites

#### Docker

Please make sure you have Docker running locally. Once you do, you can run the following command to start a local test server:

```sh
dev/nix-shell 'just backend up'
```

### Updating libxmtp rust bindings

Use the generated `xmtp_sdk` package. See [Android development rules](AGENTS.md)
for native staging, Gradle builds, and target tests.

### Dependency checks

Keep dependency locks and SHA256 verification metadata in source control.
The library, example, and plugin graphs use fixed versions. The separate
consumer keeps its included library graph in its own `gradle/library.lockfile`.

`com.android.tools:desugar_jdk_libs:2.1.5` supplies generated `java.time` APIs on
API 23 to 25. Keep desugaring on in the library and each app consumer.

After native staging, check the release library and example with frozen
dependencies:

```sh
dev/nix-shell 'cd sdks/android && ./gradlew --dependency-verification strict :library:assembleRelease :example:assembleDebug'
```

For an approved dependency change, resolve the affected tasks with
`--write-locks --write-verification-metadata sha256`. Record the dependency
reason and review the new versions and checksums. Then repeat the same tasks
with strict verification and no write flags.
