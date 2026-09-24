# Docs site

Starlight site for the self-hosted backend and SDKs. Specs are read from
`docs/specs/` and published under `/specs/`. Do not copy or edit the generated reference pages.

## Commands

Commands run in the `docs` Nix shell through the root `justfile`.

- `just install-js`: install the locked root workspace dependencies.
- `just docs browsers`: install Chromium on macOS. Linux uses the Nix browser.
- `just docs dev`: start the local site.
- `just docs build`: build the site.
- `just docs check-examples`: check source examples against the built local SDKs.
- `just docs check-agent-docs`: check all reachable public Agent SDK declarations for TSDoc.
- `just docs lint`: lint the site code and Markdown.
- `just docs format-check`: check formatting.
- `just docs format`: format the site files.
- `just docs test`: run the build-tool tests.
- `just docs test-browser`: test the composed site in Chromium.
- `just docs typecheck`: check the site and executable examples.
- `just docs compose`: combine the site and generated references.
- `just docs check`: check source links and the composed artifact.

`just docs build` and `just docs check-examples` stage local Node and WASM
bindings, then run their package tasks with the pnpm dependency graph. Do not
add a separate SDK build before either command.

## Build and check scripts

Keep scripts in `scripts/` only when the site build, CI, or local checks need
them. Keep resolved SDK examples in the developer guide and Mermaid source in
text exports. `parity/` preserves old URLs and Lighthouse score limits; it does
not freeze page text.

Oxfmt formats docs source except `.astro` files. Prettier with the Astro plugin
formats `.astro` files. `just docs format-check` checks both formatters.

## Content

Preserve retained source prose. Verify examples against the SDK source.
Use MDX only when a page needs components. Platform tabs use `syncKey="sdk"`
and the labels `Browser`, `Node`, `Kotlin`, and `Swift`. Package manager tabs
use `syncKey="pkg"`. Do not add React Native examples until they can be verified.

The public site deploys from `self-hosted`. Pull requests skip the Kotlin and
Swift references with `DOCS_SKIP_NATIVE_REFERENCES=1`; pushes build them.

## TypeScript examples

Docs uses TypeScript 6 because Astro, TypeDoc, and Twoslash need its compiler
API. The other workspace packages use TypeScript 7.

`just docs build` checks TypeScript and Twoslash examples against the local
SDK declarations. `just docs typecheck` checks Astro and executable examples.

The site tsconfig excludes `examples/`. Astro's language server forces
`isolatedModules`, which rejects the Node bindings' ambient const enums.
`examples.tsconfig.json` checks every executable example against the real SDK
declarations through `just docs check-examples` and the docs build. Keep both
checks; do not add examples to the Astro program or disable their type checks.

Put complete TypeScript programs in `examples/`. Mark a display region with
`// #region name` and `// #endregion name`. Use an empty `ts` fence with
`source="filename.ts" region="name"` to display it. Twoslash checks the whole
file and shows only that region. Missing regions and type errors fail the build.
Do not duplicate the extracted code in Markdown.

Use `twoslash` on complete inline TypeScript examples. It resolves the same
local SDK types. An inline fragment needs its real imports and typed context.
Do not add mock SDK declarations or disable type checks.
