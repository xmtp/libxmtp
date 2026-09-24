# Generated TypeScript lint

The pinned stock ubrn templates emit `any` and `as` casts. The generated SDK
bindings have 95 `no-explicit-any` findings under the root Oxlint rules. The
root Oxlint and Oxfmt configs exclude `target/sdk-generated/**` for this
reason.

`just sdk lint` checks the generated façade ID names. It runs the root
Oxlint and Oxfmt configs on the TypeScript runtime bridge that this repository
maintains. It does not report the stock generated TypeScript as linted. Remove
the exclusion when the fork templates meet the root rules.
