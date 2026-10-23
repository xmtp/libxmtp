---
name: writing-agent-docs
description: Use when creating, editing, or reviewing AGENTS.md, CLAUDE.md, skills, or linked agent guidance.
---

# Writing agent docs

Every token should change an agent's decisions.

- Keep AGENTS.md to directory purpose, essential commands, and consequential
  constraints. Shared rules belong in the nearest common parent; do not repeat
  them in children. CLAUDE.md stays a pointer: @AGENTS.md.
- Put task-specific procedures, troubleshooting, and examples in focused docs
  or skills. Link them with a clear trigger for when to read more. Prune before
  moving; relocating bloat does not fix it.
- Reuse owning specs, Just recipes, and maintained guides. Avoid copied
  requirements, command catalogs, source inventories, line numbers, generic
  advice, history, and temporary status.
- Document the general constraint, not the story of one failure. Prefer
  enforcing a rule in tooling when practical; avoid permanent instructions
  for one-off workarounds.
- Preserve critical constraints, prerequisites, and platform exceptions.
  Shortening or moving text must not weaken an obligation.
- Verify commands and claims against current files. Follow each new link:
  its target must contain accurate, useful guidance, not merely exist.
  Avoid history searches unless current sources leave a material ambiguity.
- Review the diff for lost meaning. Each removed rule should be redundant,
  obsolete, inherited, or discoverable through a relevant link. Run the
  applicable documentation checks and report any that could not run.
