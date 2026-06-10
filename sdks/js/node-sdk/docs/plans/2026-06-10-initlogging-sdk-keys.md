# `initLogging` must accept SDK option keys (camelCase), not raw `LogOptions`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax. All paths are relative to the node-sdk package root `sdks/js/node-sdk/`.

**Goal:** Make `@xmtp/node-sdk`'s `initLogging` accept the same camelCase option keys the SDK uses everywhere else (`structuredLogging` / `loggingLevel` / `stdoutLoggingLevel` / `otelEndpoint` / `resourceAttributes`), instead of silently re-exporting the raw `@xmtp/node-bindings` function whose keys are `structured` / `level` / `stdoutLevel`.

**Architecture:** Stop re-exporting the raw binding `initLogging` from `index.ts`. Add a thin SDK wrapper `initLogging(options?: LogOptions)` in `src/utils/logging.ts` that maps the SDK's camelCase `LogOptions` (a subset of `ClientOptions`) to the raw binding `LogOptions` keys — reusing the exact same mapping `createClient` already does — then calls the raw binding function. Export the wrapper from `index.ts`.

## Background (why this matters — real incident)

The SDK exposes logging options on `ClientOptions` in camelCase: `structuredLogging`, `loggingLevel`, `stdoutLoggingLevel`, `otelEndpoint`, `resourceAttributes` (see `src/types.ts`, `OtherOptions`). Inside `createClient` the SDK **maps** those to the raw binding `LogOptions` keys before calling the native layer (`src/utils/createClient.ts`):

```ts
const logOptions: LogOptions = {
  structured: options?.structuredLogging ?? false,
  level: options?.loggingLevel ?? LogLevel.Off,
  stdoutLevel: options?.stdoutLoggingLevel,
  otelEndpoint: options?.otelEndpoint,
  resourceAttributes: options?.resourceAttributes,
};
```

But the standalone `initLogging` export is the **raw binding function**, re-exported verbatim (`src/index.ts`):

```ts
export {
  ...
  initLogging,   // <-- raw @xmtp/node-bindings fn; expects { structured, level, stdoutLevel, ... }
  ...
} from "@xmtp/node-bindings";
```

So a consumer who installs logging at process boot the natural SDK way:

```ts
import { initLogging, LogLevel } from "@xmtp/node-sdk";
initLogging({ structuredLogging: true, loggingLevel: LogLevel.Info, stdoutLoggingLevel: LogLevel.Warn });
```

passes camelCase keys to a function that reads `structured`/`level`/`stdoutLevel` — **all three are silently dropped.** `init_logging` runs with `level = Off`-defaulted and `stdout_level = None`, so the stdout console level is never applied and libxmtp INFO leaks to stdout. This was diagnosed in herald-lite: `XMTP_STDOUT_LOG_LEVEL=Warn` was inert in production because `initLogging` ignored the SDK keys. The fix there was a downstream key-mapping shim; the ergonomic fix belongs in the SDK so no consumer hits this again.

TypeScript does not catch it because the raw binding `LogOptions` has `structured`/`level`/`stdoutLevel` as **optional**, so an object of camelCase keys is assignable (excess-property checks don't fire on a variable, and the camelCase keys are simply ignored extra properties).

## File Structure

- `src/utils/logging.ts` — **new.** Exports the SDK `LogOptions` type (camelCase) and the wrapper `initLogging(options?: LogOptions)` that maps to raw keys and calls the binding.
- `src/index.ts` — **modify.** Remove `initLogging` from the raw `@xmtp/node-bindings` re-export list; add `export * from "./utils/logging"`.
- `src/utils/createClient.ts` — **modify (DRY).** Import the shared mapper from `logging.ts` and use it to build `logOptions`, so the camelCase→raw mapping lives in exactly one place.
- `test/logging.test.ts` — **new.** Unit tests for the mapper + that the wrapper forwards mapped keys to the binding.

---

## Task 1: Add the SDK `LogOptions` type + `toRawLogOptions` mapper + `initLogging` wrapper

**Files:**
- Create: `src/utils/logging.ts`

- [ ] **Step 1: Write the wrapper module**

Create `src/utils/logging.ts`:

```ts
import {
  initLogging as initLoggingBinding,
  LogLevel,
  type LogOptions as RawLogOptions,
} from "@xmtp/node-bindings";

/**
 * SDK-facing logging options. These are the same camelCase keys used on
 * `ClientOptions` (see `OtherOptions` in `../types`), so the same object can be
 * passed to `Client.create`/`Client.build` and to `initLogging`.
 */
export type LogOptions = {
  /** Enable structured JSON logging. */
  structuredLogging?: boolean;
  /** Log level. Also the level exported to OTLP when `otelEndpoint` is set. */
  loggingLevel?: LogLevel;
  /**
   * Level for the stdout console layer only. Defaults to `loggingLevel`. Set to
   * `LogLevel.Warn` to quiet stdout below the OTLP export level (e.g. so a log
   * shipper does not duplicate logs already exported via OTLP).
   */
  stdoutLoggingLevel?: LogLevel;
  /** OTLP endpoint for exporting telemetry spans and correlated logs. */
  otelEndpoint?: string;
  /** Resource attributes attached to all exported telemetry. */
  resourceAttributes?: Record<string, string>;
};

/**
 * Map the SDK's camelCase logging options to the raw `@xmtp/node-bindings`
 * `LogOptions` keys (`structured`/`level`/`stdoutLevel`). This is the single
 * source of truth for that mapping, shared by `initLogging` and `createClient`.
 */
export const toRawLogOptions = (options?: LogOptions): RawLogOptions => ({
  structured: options?.structuredLogging ?? false,
  level: options?.loggingLevel ?? LogLevel.Off,
  stdoutLevel: options?.stdoutLoggingLevel,
  otelEndpoint: options?.otelEndpoint,
  resourceAttributes: options?.resourceAttributes,
});

/**
 * Install the global libxmtp logging/telemetry pipeline before any client is
 * created. Accepts the SDK's camelCase options (unlike the raw binding, which
 * this wraps). Idempotent: the first call wins; later implicit init inside
 * client creation is a no-op.
 */
export const initLogging = (options?: LogOptions): void => {
  initLoggingBinding(toRawLogOptions(options));
};
```

NOTE: confirm the raw binding `initLogging`'s arity/return at the pinned `@xmtp/node-bindings` version. At the version this SDK pins it is synchronous (`(options?: LogOptions) => void`). If the pinned binding made it `async` (returns a `Promise`), make the wrapper `async` and `return initLoggingBinding(...)`, and update Task 3's tests to `await`. Check `node_modules/@xmtp/node-bindings/dist/index.d.ts` for the `initLogging` signature before finalizing.

- [ ] **Step 2: Typecheck**

Run: `yarn typecheck` (or `tsc --noEmit` per the package's scripts)
Expected: clean. The `@/` path alias resolves `@xmtp/node-bindings` and local imports.

- [ ] **Step 3: Commit**

```bash
git add src/utils/logging.ts
git commit -m "feat(node-sdk): SDK initLogging wrapper that maps camelCase keys to raw LogOptions"
```

---

## Task 2: Export the wrapper from `index.ts` (and stop re-exporting the raw fn)

**Files:**
- Modify: `src/index.ts`

- [ ] **Step 1: Remove `initLogging` from the raw binding re-export**

In `src/index.ts`, the block `export { ... initLogging ... } from "@xmtp/node-bindings";` lists `initLogging`. Delete the `initLogging,` line from that list ONLY (leave every other re-exported name, e.g. `flushTelemetry`, `LogLevel`, `IdentifierKind`, untouched). Be careful to remove exactly one identifier and keep the list valid (no trailing/double commas).

- [ ] **Step 2: Add the wrapper export**

Near the other `export * from "./utils/*";` lines at the top of `src/index.ts` (e.g. after `export * from "./utils/messages";`), add:

```ts
export * from "./utils/logging";
```

This re-exports both `initLogging` (the wrapper) and the SDK `LogOptions` type. Confirm there is no name collision: the raw `LogOptions` type was NOT previously exported by name from `index.ts` (only via the `@xmtp/node-bindings` star/explicit list). Grep `grep -n "LogOptions" src/index.ts` — if the raw `LogOptions` is explicitly re-exported there, remove that raw re-export so the SDK `LogOptions` (camelCase) is the public one.

- [ ] **Step 3: Typecheck + confirm the public surface**

Run: `yarn typecheck`
Expected: clean. Then verify the wrapper is what's exported:
```bash
grep -n "initLogging" src/index.ts
```
Expected: NO `initLogging` in the `@xmtp/node-bindings` re-export list; it now comes from `./utils/logging`.

- [ ] **Step 4: Commit**

```bash
git add src/index.ts
git commit -m "feat(node-sdk): export the wrapped initLogging (camelCase keys) instead of the raw binding"
```

---

## Task 3: Add tests for the mapper + the wrapper

**Files:**
- Create: `test/logging.test.ts`

- [ ] **Step 1: Write the tests**

Model the file on `test/createBackend.test.ts` (vitest, `@/` import alias). Create `test/logging.test.ts`:

```ts
import { LogLevel } from "@xmtp/node-bindings";
import { describe, expect, it, vi } from "vitest";
import { initLogging, toRawLogOptions } from "@/utils/logging";

// Spy on the raw binding so we assert the EXACT options forwarded to it,
// without installing a real global subscriber (which is process-global / one-shot).
vi.mock("@xmtp/node-bindings", async (importActual) => {
  const actual = await importActual<typeof import("@xmtp/node-bindings")>();
  return { ...actual, initLogging: vi.fn() };
});

describe("toRawLogOptions", () => {
  it("maps SDK camelCase keys to raw binding keys", () => {
    expect(
      toRawLogOptions({
        structuredLogging: true,
        loggingLevel: LogLevel.Info,
        stdoutLoggingLevel: LogLevel.Warn,
        otelEndpoint: "http://collector:4317",
        resourceAttributes: { "service.name": "app" },
      }),
    ).toEqual({
      structured: true,
      level: LogLevel.Info,
      stdoutLevel: LogLevel.Warn,
      otelEndpoint: "http://collector:4317",
      resourceAttributes: { "service.name": "app" },
    });
  });

  it("defaults structured=false and level=Off when unset", () => {
    expect(toRawLogOptions()).toEqual({
      structured: false,
      level: LogLevel.Off,
      stdoutLevel: undefined,
      otelEndpoint: undefined,
      resourceAttributes: undefined,
    });
  });
});

describe("initLogging", () => {
  it("forwards mapped raw options to the binding (stdoutLevel set from stdoutLoggingLevel)", async () => {
    const { initLogging: rawInit } = await import("@xmtp/node-bindings");
    initLogging({ stdoutLoggingLevel: LogLevel.Warn, loggingLevel: LogLevel.Info });
    expect(rawInit).toHaveBeenCalledWith(
      expect.objectContaining({ stdoutLevel: LogLevel.Warn, level: LogLevel.Info }),
    );
    // The bug this fixes: passing SDK keys must NOT leave stdoutLevel undefined.
    const callArg = vi.mocked(rawInit).mock.calls.at(-1)?.[0];
    expect(callArg?.stdoutLevel).toBe(LogLevel.Warn);
  });
});
```

NOTE: if the mock factory shape differs from this repo's convention, mirror whatever `test/Client.test.ts` does to mock `@xmtp/node-bindings` (it already references `initLogging`/`loggingLevel`). Keep the assertion intent: the wrapper forwards `stdoutLevel` (raw key) derived from `stdoutLoggingLevel` (SDK key).

- [ ] **Step 2: Run the tests**

Run: `yarn test test/logging.test.ts` (or the repo's vitest invocation)
Expected: all pass. If the `initLogging` forwarding test fails because the wrapper is async (per Task 1 NOTE), `await initLogging(...)` and re-run.

- [ ] **Step 3: Commit**

```bash
git add test/logging.test.ts
git commit -m "test(node-sdk): initLogging maps SDK keys to raw LogOptions"
```

---

## Task 4: DRY — reuse the shared mapper inside `createClient`

**Files:**
- Modify: `src/utils/createClient.ts`

- [ ] **Step 1: Replace the inline mapping with the shared mapper**

In `src/utils/createClient.ts`, the inline `const logOptions: LogOptions = { structured: ..., level: ..., stdoutLevel: ..., otelEndpoint: ..., resourceAttributes: ... };` duplicates the mapping now owned by `logging.ts`. Replace it with a call to the shared mapper so the two paths can never drift:

```ts
import { toRawLogOptions } from "@/utils/logging";
// ...
const logOptions = toRawLogOptions(options);
```

`options` here is the `ClientOptions` passed to `createClient`; it is a superset of the SDK `LogOptions` (it carries `structuredLogging`/`loggingLevel`/`stdoutLoggingLevel`/`otelEndpoint`/`resourceAttributes` plus client-only keys). `toRawLogOptions` reads only the logging keys, so passing the full `ClientOptions` is fine — confirm the param type of `toRawLogOptions` (`LogOptions`) is structurally satisfied by `ClientOptions` (it is, since `OtherOptions` is part of `ClientOptions`). If TypeScript complains about excess/missing properties, widen `toRawLogOptions`'s param to `LogOptions` via a structural pick rather than narrowing `ClientOptions`.

Remove the now-unused inline `LogOptions` import from `@xmtp/node-bindings` in this file IF it is no longer referenced after the change (the raw `LogOptions` type may still be needed elsewhere in the file — check before deleting the import; let `tsc`/lint guide removal).

- [ ] **Step 2: Typecheck + run the client tests**

Run: `yarn typecheck && yarn test test/Client.test.ts`
Expected: clean + pass. `createClient`'s behavior is unchanged (same raw `logOptions` produced); this is a pure refactor to a single mapping source.

- [ ] **Step 3: Commit**

```bash
git add src/utils/createClient.ts
git commit -m "refactor(node-sdk): build createClient logOptions via the shared toRawLogOptions mapper"
```

---

## Task 5: Full verification

**Files:** verify only.

- [ ] **Step 1: Build + typecheck + lint + test the package**

```bash
yarn build        # or the package's build script
yarn typecheck
yarn lint         # if present (eslint/biome)
yarn test
```
Expected: build emits the new `logging` exports; typecheck clean; lint clean; all tests pass including `test/logging.test.ts`.

- [ ] **Step 2: Confirm the public API shape**

Verify the published types expose the wrapped `initLogging`:
```bash
grep -n "initLogging\|LogOptions" dist/index.d.ts
```
Expected: `initLogging` typed to accept the SDK `LogOptions` (camelCase: `stdoutLoggingLevel`, not `stdoutLevel`), and `LogOptions` exported with camelCase fields.

- [ ] **Step 3: (optional) E2E smoke**

With a built package, in a node script: `import { initLogging, Client, LogLevel } from "@xmtp/node-sdk"; initLogging({ loggingLevel: LogLevel.Info, stdoutLoggingLevel: LogLevel.Warn });` then create a client and provoke a libxmtp op — confirm stdout shows only WARN+ libxmtp lines (no INFO `xmtp_mls`/`xmtp_api`). Before the fix, INFO leaked despite `stdoutLoggingLevel: Warn`.

---

## Self-Review

**1. Coverage** — the bug ("standalone `initLogging` is the raw binding fn; SDK camelCase keys are silently dropped") is fixed by Tasks 1–2 (wrapper that maps keys + exported in place of the raw fn). Task 3 locks the mapping with tests (including the exact `stdoutLevel`-set assertion that was the failure). Task 4 removes the duplicate mapping so `createClient` and `initLogging` can't drift. Task 5 verifies the public surface now takes camelCase.

**2. Placeholders** — none. The two version-dependent points (binding `initLogging` sync-vs-async; whether the file still needs the raw `LogOptions` import after the refactor) are explicit read-then-decide NOTES, not TBDs.

**3. Consistency** — `toRawLogOptions` is defined once in `logging.ts`, used by both `initLogging` (Task 1) and `createClient` (Task 4); the SDK `LogOptions` type mirrors `OtherOptions` in `src/types.ts` (`structuredLogging`/`loggingLevel`/`stdoutLoggingLevel`/`otelEndpoint`/`resourceAttributes`). Raw keys (`structured`/`level`/`stdoutLevel`) appear only inside the mapper's return.
