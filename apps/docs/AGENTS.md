# Docs site

Starlight site. Specs come from `docs/specs/`; never edit generated references.
Follow [docs validation requirements](../../docs/AGENTS.md) before submission.

- `dev/nix-shell 'just docs build'` builds local SDKs once through the task graph;
  do not add a separate SDK build. `just docs typecheck`, `just docs lint`, and
  `just docs test` (through Nix) check examples, source, and build tools.
- Keep scripts only when builds, CI, or local checks need them. `parity/`
  preserves old URLs and Lighthouse limits, not page text.
- Preserve retained prose; verify examples against public SDK exports. Never
  mock SDK declarations or disable example type checks. Do not copy extracted
  example code into Markdown.
- Use MDX only for components. SDK tabs use `syncKey="sdk"` and labels Browser,
  Node, React Native, Kotlin, Swift (optional Agent last); package tabs use `pkg`.
- Read [authoring](AUTHORING.md) for executable examples, Twoslash, formatting,
  and composed-site browser checks; [validation](VALIDATION.md) for build artifacts.
