# Contributing

Thank you for considering contributing to this repo. Community contributions like yours are key to the development and adoption of XMTP. Your questions, feedback, suggestions, and code contributions are welcome.

## ❔ Questions

Have a question about how to build with XMTP? Ask your question and learn with the community in the [Q&A discussion forum](https://github.com/orgs/xmtp/discussions/categories/q-a).

## 🐞 Bugs

Report bugs as [GitHub Issues](https://github.com/xmtp/xmtp-android/issues/new?assignees=&labels=bug&template=bug_report.yml&title=Bug%3A+). Please confirm that there isn't an existing open issue about the bug and include detailed steps to reproduce the bug.

## ✨ Feature requests

Submit feature requests as [GitHub Issues](https://github.com/xmtp/xmtp-android/issues/new?assignees=&labels=enhancement&template=feature_request.yml&title=Feature+request%3A+). Please confirm that there isn't an existing open issue requesting the feature. Describe the use cases this feature unlocks so the issue can be investigated and prioritized.

## 🔀 Pull requests

PRs are encouraged, but consider starting with a feature request to temperature-check first. If the PR involves a major change to the protocol, the work should be fleshed out as an [XMTP Improvement Proposal](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-0-purpose-process.md) before work begins.

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
script/local
```

### Updating libxmtp rust bindings

Please see [LibXMTP Kotlin README](https://github.com/xmtp/xmtp-android/blob/main/library/src/main/java/README.md).
