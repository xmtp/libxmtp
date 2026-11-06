# Testing conventions

Don't write tests that reference other example app code, and don't write comments that reference features in other example apps.

These rules keep a valid change from breaking a test:

- Assert through what screens use: store state, the helpers screens call (`visibleSuggestions`, `statusPick`, `canJoin`), and the port calls whose presence is the behavior. Do not assert private fields, the identity of a state object, an order nothing shows, or leftover state on a closed store.
- Assert copy through its constant (`SessionCopy.signOutFailed`, `ConnectionCopy.reconnecting`), never as English text. For copy built from values, assert the values in it (the handle, the minimum, the dates) and that the branches differ. Copy lives in `copy.ts` or a `*Copy` object, not inline in `src/core` logic.
- Count fake calls only when the count is the behavior ("no SDK call after the chat closed", "saved once per position"). Do not assert exact call lists, call arguments, or the order of independent calls. For notifications, assert none or some, not an exact number.
- Inject a fault by content, not by call number: `failOnce((drafts) => drafts.some((entry) => entry.phase === "accepted"))`, not `failWrite = 2`.
- A tuning value (page size, threshold, limit, swipe distance, retry delay) comes from its named constant or from what the code reports, never a literal.
- A negative case must reach the guard it names: let the other guards pass, and assert what only that guard produces. Otherwise it fails, or goes silent, when an unrelated guard changes.
- Fakes resolve at once and apply their own effects. No timer that waits real time. Drain with `settle()` and hold work with `deferred()` from `src/core/testing.ts`; a zero-delay macrotask is fine. Close long-lived objects in `afterEach`. Record values inside a fake callback and assert after the call returns.
- Pin each saved format (index JSON, profile paths, preference file names, journal shape) with a literal fixture the RN app owns. A change to it must fail a test, because installed apps read it.
- Test a rule at one layer. Add a pure-function test only for cases the store test cannot reach cheaply, and put the variants of one rule in one table.
- Do not export a function, option or method only for a test. When a test is the last caller of production code, delete both.

Prove each new test fails under a named behavioral mutation, then restore it
and show it passes. Record the mutation in the change description.
