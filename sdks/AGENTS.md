# XMTP SDKs

Shared behavior belongs in Rust/core and `crates/xmtp_sdk`; platform packages wrap
it. Never edit generated bindings. Keep native streams open through retryable
faults; core owns ordered recovery and durable delivery progress.

For JS/TS SDKs (`node`, `browser`, `agent`, `react-native`):

- Use the root pnpm workspace. `dev/nix-shell 'just install-js'` installs it;
  `dev/nix-shell 'just js sdk-products [node|browser|react-native]'` stages products.
- Use the task graph; never `--parallel` or `--no-sort`. Agent consumes Node's
  `dist`. Read [package preparation](DEVELOPMENT.md) when changing build routes.
- Client creation requires explicit `backend`; Browser also requires `storage`.
  `env` labels database files, never selects a backend. Preserve the API cache
  key `<backendUrl>|<appVersion>`.
- Tests need `just backend up` through Nix unless documented otherwise.
- Iteration acknowledges the previous message on the next read; close/end do
  not acknowledge. Callback mode acknowledges only after callback success and
  cannot also iterate. Keep typed failures; never automatically replace failed
  readers. Read [delivery rules](DEVELOPMENT.md#durable-message-delivery) for stream work.
