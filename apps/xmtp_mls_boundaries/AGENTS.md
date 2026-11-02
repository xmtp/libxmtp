# xmtp_mls_boundaries

`check-boundaries`, run through `dev/check-boundaries`, enforces
`crates/xmtp_mls/boundaries.toml`. The `path` tier reads `use` items and
`crate::`/`super::`/`self::` paths with `syn`. `--full` adds the `call` tier
from two rust-analyzer SCIP indexes built with every feature on, native with
`test` on and `wasm32-unknown-unknown` with `test` off; it counts every
reference to an item, method calls included, in the builds that compile it.
Test suites (`cfg(test)` modules holding tests) are not read by either tier;
`cfg(test)` helpers and hook modules are. Items inside xmtp_common's item
macros (`if_native!`, `if_wasm!`, `if_only_test!` and the like) are read under
the macro's `cfg`; a module declared inside a block or in any other macro's
arguments is refused, as are a local macro or an import named like one that
never evaluates its arguments (`stringify!` and the like), unless it imports
that macro from `std`, `core` or `alloc`, and an alias named `std`, `core` or
`alloc`. Glob imports and `#[macro_use]` crates are not checked for such names. The method calls neither build compiles
(native code with `test` off, wasm test code) are read only by the path
tier; `[[uncovered]]` in `boundaries.toml` lists each by file and method,
and `--base` refuses an added entry. Such a call is refused when an impl or
trait the tier reads defines a method of that name, since the call could then
be an edge; methods that macros generate are not seen.

```sh
dev/nix-shell 'dev/check-boundaries --report'
dev/nix-shell 'dev/check-boundaries --full'   # about 20 min and 10 GB
dev/nix-shell 'dev/agent-run cargo test -p xmtp_mls_boundaries'
dev/nix-shell 'dev/agent-run cargo clippy -p xmtp_mls_boundaries --all-targets --no-deps -- -D warnings'
```

This package is not a workspace default member; run its checks explicitly.
CI runs the path tier only. The call tier fails closed: it refuses an index
from another tool, without definition bodies or without a stated position
encoding, an index missing a module file its build compiles, a method call
it does not resolve at the call's position (in the arguments of macros that
may evaluate them too), and an `[[uncovered]]` list other than the measured
calls. Each new failure mode needs a fixture test in `src/tests.rs`
that fails.
