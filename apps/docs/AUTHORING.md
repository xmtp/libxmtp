# Site authoring

Keep scripts in `scripts/` only when the site build, CI, or local checks need
them. Keep resolved SDK examples in the developer guide and Mermaid source in
text exports. `parity/` preserves old URLs and Lighthouse score limits; it does
not freeze page text.

Oxfmt formats docs source except `.astro` files. Prettier with the Astro plugin
formats `.astro` files. `just lint-js-format` checks both formatters.

## Content

Preserve retained source prose. Verify examples against the SDK source.
Use MDX only when a page needs components. Platform tabs use `syncKey="sdk"`
and the labels `Browser`, `Node`, `React Native`, `Kotlin`, and `Swift`. A guide that also covers
the Agent SDK can add `Agent` as the last tab. Package manager tabs
use `syncKey="pkg"`. Verify React Native examples against the generated public declarations, including the optional `/react` entry. The docs route stages declarations without the full mobile build closure.

The post-merge workflow (`post-merge.yml`) deploys the public site on each push to `main` and
builds the Kotlin and Swift references in its own jobs. The CI merge run does
not build the site. Pull requests skip the native references with
`DOCS_SKIP_NATIVE_REFERENCES=1`.
Use `dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs test-browser'`
when testing that composed site.

## TypeScript examples

Docs uses TypeScript 6 because Astro, TypeDoc, and Twoslash need its compiler
API. The other workspace packages use TypeScript 7.

`just docs build` checks TypeScript and Twoslash examples against the local
SDK declarations. `just docs typecheck` checks Astro and executable examples.

The site tsconfig excludes `examples/`. Check executable examples separately
against the public SDK package exports.
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
