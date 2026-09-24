---
neverApprove:
  - .macroscope/**
  - docs/self-hosted/agent-context.md
  - docs/specs/**
  - .github/CODEOWNERS
  - apps/backend/migrations/**
  - crates/xmtp_db/migrations/**
---

# Auto-approval rules

Use the full PR diff and the results of Spec Compliance, Security, Code
Quality, and Correctness. Do not auto-approve when an applicable check reports
an unresolved material finding, fails, or stops before it completes. Do not
interpret a skipped or missing applicable review as a clean result.

Do not auto-approve changes to authentication, identity, cryptography, MLS
validation, group permissions, key or secret handling, or privacy controls.
Require human review for changes to wire behavior, public binding or SDK
contracts, durable data, migrations, or recovery and ordering guarantees.

Do not auto-approve a behavior change that conflicts with an approved spec or
needs a spec amendment that has not been reviewed by an owner. Do not
auto-approve a change that removes or weakens tests, validation, logging
redaction, or CI checks for affected behavior.
