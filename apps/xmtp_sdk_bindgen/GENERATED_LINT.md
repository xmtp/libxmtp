# Generated TypeScript lint

The pinned stock ubrn templates emit `any` and `as` casts. The generated SDK
bindings have 95 `no-explicit-any` findings under the root Oxlint rules. The
root Oxlint and Oxfmt configs exclude that stock output (`xmtp_sdk*.ts`,
`binding.ts`, and the WASM snippets in each `target/sdk-generated` tree) for
this reason.

`just sdk lint` checks generated façade ID names, the runtime, and the
generated bridge `*.gen.ts` and `*.gen.test.ts` files. The runtime in
`apps/xmtp_sdk_bindgen/runtime/ts` imports the binding and the files that the
generator writes beside it, so the lint reads its copy in each generated tree:
Oxlint checks every copied runtime file, and `tsc` typechecks the Node and
browser copies against the real binding and the `@ubjs` packages that each
tree links in its `node_modules`. Run
`just sdk generate` first. The Oxlint rule `typescript/consistent-type-assertions`
(an override in `.oxlintrc.json`) rejects type assertions in the bridge, its
copies, and the generated bridge modules. Stock ubrn output stays excluded until its templates meet the root
rules.
