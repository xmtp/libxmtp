# Improving Error Handling

## Current Situation

libxmtp uses Rust enums plus the [thiserror crate](https://docs.rs/thiserror/latest/thiserror/) to define errors like: StorageError, GroupError, APIError, etc.

Today these Rust errors are generally turned into generic JavaScript errors in the bindings.

Our JavaScript/TypeScript SDK consumers mainly see:

- Error instances in JS that are of type `GenericFailure`
- Error messages, but no stable error codes or structured shapes

This makes it hard to:

- Categorize errors in JS / TS
- Catch errors depending on their context
- Map errors to documentation or troubleshooting guides
- Translate errors for frontend UIs or mobile views
- Add error-specific behavior (retry, user facing messaging, etc.)

### Examples

**Problem 1: Cryptic messages**

> [Error: synced 1 messages, 0 failed 1 succeeded from cursor Some(20285901)] {code: 'GenericFailure'}

Technical jargon makes errors incomprehensible to end-users.

**Problem 2: Generic error codes**

> [Error: Unknown signer] { code: 'GenericFailure' }

All errors share the same code, preventing type-specific error handling in TypeScript.

**Problem 3: Missing structured data**

> [Error: Cannot register a new installation because the InboxID 3f3b0225eebf5006d962fae4d4a7cba68ba661756bd55abc4d07b80de4a31c0e has already registered 23/5 installations. Please revoke existing installations first.] {code: 'GenericFailure'}

Important values (installation limit: 5) are embedded in strings rather than accessible as properties.

## How Our Partners Currently Handle Errors

Without structured error types, partners resort to fragile string parsing.

**Real-World Example:**

```ts
const errorMessage = error instanceof Error ? error.message : String(error);
if (errorMessage.toLowerCase().includes("revoke existing installations")) {
  // Handle max installations error
}
```

**Why this is problematic:**

- Breaks when error message wording changes
- No type safety for error instances
- Forces developers to parse production logs to discover which strings to match

## What Partners Should Be Able to Do

```ts
try {
  // ...
} catch (error) {
  if (error instanceof MaxInstallationsError) {
    // TypeScript knows "error.details.currentCount" and "error.details.maxCount" exist
    showRevokeInstallationsUI(
      error.details.currentCount,
      error.details.maxCount
    );
  }
}
```

**Alternative approach:**

Using a unique `code` property is easier to implement when writing glue code that maps Rust errors to TypeScript errors. This could be a first step that provides immediate value without preventing us from later evolving these into full error class instances.

```ts
if (error.code === "MaxInstallationsError") {
  showRevokeInstallationsUI(error.details.currentCount, error.details.maxCount);
}
```

## Great Examples of Error Handling

Popular libraries and tools in the JavaScript/TypeScript ecosystem demonstrate well-designed error handling patterns worth emulating.

### Zod

[Zod’s validation errors](https://zod.dev/basics?id=handling-errors) are a good model: machine readable, structured, self-explaining, and capable of carrying parameters like limits (`minimum`).

**Example:**

```ts
import z from "zod";
const schema = z.string().min(5);
schema.parse("hi"); // throws ZodError
```

```json
ZodError: [
  {
    "origin": "string",
    "code": "too_small",
    "minimum": 5,
    "inclusive": true,
    "path": [],
    "message": "Too small: expected string to have >=5 characters"
  }
]
```

### Next.js

Next.js enhances error messages by including links to their documentation:

**Example:**

> Build optimization failed: found page without a React Component as default export in
> pages/index.test
> See https://nextjs.org/docs/messages/page-without-valid-component for more info.

Each documentation page provides:

- Why This Error Occurred
- Possible Ways to Fix It

### TypeScript

TypeScript compiler errors are prefixed, enabling easy source attribution in mixed error logs.

**Example:**

> TS1035: Only ambient modules can use quoted names.

Through the "TS" prefix and numeric codes, errors can also be easily identified. There are also websites that explain each error based on the code:

- https://typescript.tv/errors/#ts1035
- https://ts-error-translator.vercel.app/

The VS Code extension [Pretty TypeScript Errors](https://marketplace.visualstudio.com/items?itemName=yoavbls.pretty-ts-errors) with 1.8 million installations demonstrates the value of structured error codes by rendering these documentation links inline.

## Desired State

- Error codes are uniform across all platforms (Swift, Kotlin, TypeScript), ensuring consistent language across protocol and client teams
- Errors are instances of specific classes (e.g., `MaxInstallationsError` instead of `GenericFailure`)
- Validation errors contain structured properties (e.g., `maxCount`, `currentCount`) accessible programmatically
- Error messages include links to documentation for troubleshooting guidance
- Error messages are prefixed to indentify the failing component
- Error strack traces reveal which code path has thrown the error

## Proposal

The biggest impact comes from mapping the existing Rust errors into JavaScript errors that carry a `code` directly traceable to the original Rust variant, for example `GroupError::NotFound`. This lets JavaScript developers catch errors in a precise way (`if (error.code === "GroupError::NotFound")`), ensures SDKs and the Core Protocol team use the same vocabulary, and makes it straightforward to locate every spot in libxmtp where that error originates.

### Implementation Approach

Ry proposed a fantastic approach, that leverages the existing WASM bindings infrastructure:

1. Introduce a custom `WasmError` on the JS side
2. Make bindings (browser WASM and Node) map Rust errors into this custom error type and expose a consistent shape with a `code` property

The Rust bindings already use trait implementations and match expressions to convert Rust enums into JavaScript errors. For each variant, we can construct a `WasmError` with the appropriate fields. Rust’s exhaustiveness on `match` ensures that if new error variants are added in libxmtp, the bindings will stop compiling until the mapping is updated. This makes it easy to run an LLM once over the file to add the new variants.

The path is incremental: We can start with a single `WasmError` class with a strong `code` field, then evolve to fully typed error classes (see Action Plan).

### Namespaces

Use namespaced codes in the format `EnumName::VariantName`:

- `GroupError::NotFound`
- `GroupError::MissingSequenceId`

**Benefits:**

- Enables quick mapping to Rust source by searching for the exact code
- Prevents collisions when different error enums share variant names (e.g., multiple `NotFound` variants)

## Action Plan

| Action                                                                                                                          | Example                                                                                            | Benefit                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Write translation layer code that maps libxmtp errors to `WasmError` with an identifiable `code`                             | Moving from `GenericFailure` to `GroupError::NotFound`                                             | Allows responding differently to different types of errors                                                                                                           |
| 2. Prefix all error messages with the component that throws it                                                                  | Moving from generic `Error:` prefix to `xmtp_mls:`                                                 | Allows partners to identify in logs which errors are coming from us; allows us to know if a submitted error was caused by us or an external system                   |
| 3. Move from `WasmError` to specific error classes that still have a `code` property but can also contain additional properties | Moving from `WasmError` to `GroupErrorNotFound`                                                    | Allows partners to use `instanceof` checks in TypeScript which enable type guards so custom properties from error classes can be discovered through auto-completion  |
| 4. Add additional properties (like installation limits) to specific errors                                                      | Adding `error.details.maxCount`                                                                    | Allows partners to build custom error messages, even in different languages                                                                                          |
| 5. Build AI workflow that submits newly added errors to our documentation pages                                                 | Claude Code GitHub Action to update docs with new Error classes                                    | Allows us to build an error inventory                                                                                                                                |
| 6. Add links to docs pages in error messages                                                                                    | Moving from `Unknown signer` to `Unknown signer, see: https://docs.xmtp.org/errors/unknown-signer` | Allows us to track the most hit errors and to provide guidance on how to fix the error and get in touch with partners through our comments section on the docs pages |

## Future Improvements

As of today, multiple branches in libxmtp can produce the same message string with the same high level variant (Example: `GroupError::NotFound`). In an ideal world each logical error condition would have a unique code or distinct metadata that unambiguously points back to a single code path.
