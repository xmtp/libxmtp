# Generated TypeScript lint

The pinned stock ubrn templates emit `any` and `as` casts. The generated SDK
bindings have 95 `no-explicit-any` findings under the root Oxlint rules. The
root Oxlint and Oxfmt configs exclude `target/sdk-generated/**` for this
reason.

`just sdk lint` checks generated façade ID names, the bridge runtime, and the
generated bridge `*.gen.ts` and `*.gen.test.ts` files. The root Oxlint config
excludes stock ubrn output and includes these files.
It typechecks generated bridge modules and rejects TypeScript type assertions
in the bridge. Stock ubrn output
stays excluded until its templates meet the root rules.
