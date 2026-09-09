# Docs site

Starlight site for the self-hosted backend and SDKs. Specs are read from
`docs/specs`. Do not copy or edit the generated reference pages.

## Commands

Commands run in the `docs` Nix shell through the root `justfile`.

- `just docs install`: install the locked dependencies.
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
- `just docs compose`: combine the site and generated references.
- `just docs check`: check source links and the composed artifact.

## Build and check scripts

Keep scripts only when the site build, CI, or local checks need them.
Do not keep one-time migration checks or copies of old page prose.

- `compose.mjs` and `references.mjs`: assemble the deployment artifact and native API references.
- `check-site.mjs` and `validation-lib.mjs`: check links, redirects, native assets, and LLM exports.
- `check-serve.mjs`: serve the composed artifact for browser and Lighthouse tests.
- `check-lighthouse.mjs`: check accessibility in deployment CI; check performance before DNS cutover.
- `check-search.mjs` and `search-config.mjs`: define search regression cases and ranking settings.
- `check-ts-regions.mjs`, `example-config.mjs`, and `example-regions.mjs`: check SDK examples and render their source regions.
- `typedoc-validation.mjs`: fail API reference builds on TypeDoc warnings or errors.

The files in `parity/` keep old URLs working and set Lighthouse score limits.
They do not freeze page text or require access to the old site.

The docs package has its own Prettier configuration. `nix fmt` continues to
format the SDKs; `just docs format-check` checks the site, including Astro and MDX.

## Content

Preserve retained source prose. Verify examples against the SDK source.
Use MDX only when a page needs components. Platform tabs use `syncKey="sdk"`
and the labels `Browser`, `Node`, `Kotlin`, and `Swift`. Package manager tabs
use `syncKey="pkg"`. Do not add React Native examples until they can be verified.

The public site deploys from `main`. This project targets `self-hosted`.
DNS cutover is a separate task.

## TypeScript examples

Run `just js build` before building the docs. The examples resolve the local
SDK declaration files. The docs build runs TypeScript and Twoslash checks.

Put complete TypeScript programs in `examples/`. Mark a display region with
`// #region name` and `// #endregion name`. Use an empty `ts` fence with
`source="filename.ts" region="name"` to display it. Twoslash checks the whole
file and shows only that region. Missing regions and type errors fail the build.
Do not duplicate the extracted code in Markdown.

Use `twoslash` on complete inline TypeScript examples. It resolves the same
local SDK types. An inline fragment needs its real imports and typed context.
Do not add mock SDK declarations or disable type checks.
