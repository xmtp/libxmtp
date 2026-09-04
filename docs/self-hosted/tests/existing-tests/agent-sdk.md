# JavaScript Agent SDK test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `parseLogLevel :: should parse lowercase log levels` | Vitest sync; six values | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `parseLogLevel :: should parse uppercase log levels` | Vitest sync; six values | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `parseLogLevel :: should parse properly cased log levels` | Vitest sync; six values | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `parseLogLevel :: should parse mixed case log levels` | Vitest sync; two values | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `parseLogLevel :: should return null for invalid log levels` | Vitest sync; four values | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `getValidLogLevels :: should return all valid log levels` | Vitest sync | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/debug/log.test.ts` | `getValidLogLevels :: should return a new array each time` | Vitest sync | `AGENTSDK-REQ-001` |
| `sdks/js/agent-sdk/src/user/NameResolver.test.ts` | `NameResolver > caching behavior :: should return address immediately without API call for valid addresses` | Vitest async; mocked global fetch | `AGENTSDK-REQ-002` |
| `sdks/js/agent-sdk/src/user/NameResolver.test.ts` | `NameResolver > caching behavior :: should cache resolved names and not make duplicate API calls` | Vitest async; two calls; mocked response | `AGENTSDK-REQ-002` |
| `sdks/js/agent-sdk/src/util/AttachmentUtil.test.ts` | `AttachmentUtil > createRemoteAttachmentFromFile :: creates a remote attachment` | Vitest async; File and upload callback | `AGENTSDK-REQ-003` |
| `sdks/js/agent-sdk/src/util/AttachmentUtil.test.ts` | `AttachmentUtil > Round-trip test :: encrypts and decrypts a file` | Vitest async; mocked fetch | `AGENTSDK-REQ-003` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > erc20Abi :: contains transfer, balanceOf, and decimals` | Vitest sync | `AGENTSDK-REQ-004` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createERC20TransferCalls :: returns a valid WalletSendCalls object` | Vitest sync; 1,000,000 units | `AGENTSDK-REQ-004` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createERC20TransferCalls :: encodes transfer data correctly` | Vitest sync; selector 0xa9059cbb | `AGENTSDK-REQ-004` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createERC20TransferCalls :: includes description in metadata` | Vitest sync | `AGENTSDK-REQ-004` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createERC20TransferCalls :: converts chain ID to hex` | Vitest sync; chain 8453 | `AGENTSDK-REQ-004` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createNativeTransferCalls :: returns a valid WalletSendCalls object` | Vitest sync; 1 ETH | `AGENTSDK-REQ-005` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createNativeTransferCalls :: does not set data field` | Vitest sync; 1 wei | `AGENTSDK-REQ-005` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > createNativeTransferCalls :: includes description in metadata` | Vitest sync; 0.5 ETH | `AGENTSDK-REQ-005` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > getERC20Balance :: reads balance from the chain` | Vitest async; mocked readContract | `AGENTSDK-REQ-006` |
| `sdks/js/agent-sdk/src/util/TransactionUtil.test.ts` | `TransactionUtil > getERC20Decimals :: reads decimals from the chain` | Vitest async; mocked readContract | `AGENTSDK-REQ-006` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > fromSelf :: should return false for messages not from self` | Vitest async integration | `AGENTSDK-REQ-007` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > fromSelf :: should return true for messages from self` | Vitest async integration | `AGENTSDK-REQ-007` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > hasContent :: should return true for messages with defined content` | Vitest async; text | `AGENTSDK-REQ-007` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > hasContent :: should return false for messages with no content` | Vitest async; unregistered TestCodec | `AGENTSDK-REQ-007` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isDM :: should return true for DM conversations` | Vitest async | `AGENTSDK-REQ-008` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isDM :: should return false for group conversations` | Vitest async | `AGENTSDK-REQ-008` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroup :: should return true for group conversations` | Vitest async | `AGENTSDK-REQ-008` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroup :: should return false for DM conversations` | Vitest async | `AGENTSDK-REQ-008` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupAdmin :: should return true when sender is a group admin` | Vitest async; assigned admin | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupAdmin :: should return false when sender is not a group admin` | Vitest async; ordinary member | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupAdmin :: should return false when conversation is not a group` | Vitest async; DM | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupSuperAdmin :: should return true when sender is a group super admin` | Vitest async; creator | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupSuperAdmin :: should return false when sender is not a group super admin` | Vitest async; member | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupSuperAdmin :: should return false when conversation is not a group` | Vitest async; DM | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > isGroupSuperAdmin :: should return false when sender is regular admin but not super admin` | Vitest async | `AGENTSDK-REQ-009` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > usesCodec :: should return true for messages using a custom codec` | Vitest async; registered TestCodec; type assertion | `AGENTSDK-REQ-010` |
| `sdks/js/agent-sdk/src/core/filter.test.ts` | `Filters > usesCodec :: should return false for messages using a different codec` | Vitest async; text | `AGENTSDK-REQ-010` |
| `sdks/js/agent-sdk/src/core/MessageContext.test.ts` | `MessageContext :: should properly type the content when using reply as input` | Vitest async integration and compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.reconnect.test.ts` | `Agent reconnect :: should reconnect after a mid-stream disconnect` | Vitest async; Toxiproxy; 5-second outage; 10-second abort | `AGENTSDK-REQ-013` |
| `sdks/js/agent-sdk/src/core/Agent.reconnect.test.ts` | `Agent reconnect :: should reconnect when start() fails initially` | Vitest async; proxy initially down; 5-second delay | `AGENTSDK-REQ-013` |
| `sdks/js/agent-sdk/src/core/Agent.reconnect.test.ts` | `Agent reconnect :: should emit unhandledError on stream disconnect` | Vitest async; proxy left down; 10-second abort | `AGENTSDK-REQ-013` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: infers additional content types from given codecs` | Vitest sync compile-time assertion; asserts only `Agent<BuiltInContentTypes>` despite the source title | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types the content in message event listener` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'attachment' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'text' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'reaction' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'reply' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'group-update' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: should have proper types when using type predicates in 'unknownMessage' event` | Vitest sync; four predicate branches | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: should have proper types when using type predicates in 'conversation' event` | Vitest sync; DM and Group branches | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'start' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > types :: types content for 'stop' events` | Vitest sync compile-time assertion | `AGENTSDK-REQ-011` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should sync conversations and start listening` | Vitest async integration | `AGENTSDK-REQ-012` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should not start twice if already listening` | Vitest async; two calls | `AGENTSDK-REQ-012` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should auto-restart after a startup failure` | Vitest async; first conversation stream call throws | `AGENTSDK-REQ-012` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should filter messages from the agent itself (same senderInboxId)` | Vitest async; local DM; waitFor | `AGENTSDK-REQ-014` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should filter reaction messages from the agent itself` | Vitest async; self and remote reactions; waitFor | `AGENTSDK-REQ-014` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should emit 'group-update' events for group update messages` | Vitest async; add-admin update | `AGENTSDK-REQ-014` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > start :: should emit generic 'message' event for all message types` | Vitest async; group update, text, reaction, and reply | `AGENTSDK-REQ-014` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > conversation events :: should emit 'conversation' events for new conversations` | Vitest async; DM then Group; ordered assertions | `AGENTSDK-REQ-015` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > conversation events :: should emit specific 'dm' events for direct messages` | Vitest async | `AGENTSDK-REQ-015` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > conversation events :: should emit specific 'group' events for Group conversations` | Vitest async | `AGENTSDK-REQ-015` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > use :: should add middleware and return the agent instance` | Vitest sync | `AGENTSDK-REQ-016` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > use :: should execute middleware when processing messages` | Vitest async; one remote text | `AGENTSDK-REQ-016` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > use :: should filter self messages before they reach middleware` | Vitest async; self and remote path | `AGENTSDK-REQ-016` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > use :: should continue to next middleware when next() is called` | Vitest async; three middleware by two texts; exact order | `AGENTSDK-REQ-016` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > use :: should stop the processing chain when the middleware returns` | Vitest async; text passes and reply stops | `AGENTSDK-REQ-016` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > emit :: should emit 'text' and allow sending a reply via context` | Vitest async; peer sync and readback | `AGENTSDK-REQ-017` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > stop :: should stop listening and emit stop event` | Vitest async | `AGENTSDK-REQ-018` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > create :: should set appVersion to include package version by default` | Vitest async; default | `AGENTSDK-REQ-018` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > create :: should allow custom appVersion to override default` | Vitest async; custom-app/1.0.0 | `AGENTSDK-REQ-018` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > errors.use :: propagates error, transforms, recovers, and resumes remaining middleware` | Vitest async; exact normal and error order | `AGENTSDK-REQ-019` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > errors.use :: doesn't emit when a middleware returns early` | Vitest async; normal queue short circuit | `AGENTSDK-REQ-019` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > errors.use :: can end an error queue when returning` | Vitest async; error queue short circuit | `AGENTSDK-REQ-019` |
| `sdks/js/agent-sdk/src/core/Agent.test.ts` | `Agent > errors.use :: emits an error if no custom error middleware is registered` | Vitest async; default unhandledError | `AGENTSDK-REQ-019` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > constructor :: logs an initial health report` | Vitest sync; fake timers; console spy | `AGENTSDK-REQ-020` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > constructor :: uses default config values` | Vitest sync; advance 60 seconds | `AGENTSDK-REQ-020` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > constructor :: accepts a custom reporting interval` | Vitest sync; 5-second interval; advance 15 seconds | `AGENTSDK-REQ-020` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > constructor :: calls custom health report handler instead of logging` | Vitest sync; callback and 10-second advance | `AGENTSDK-REQ-020` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > constructor :: disables health reports when reporting interval is 0` | Vitest sync; advance 120 seconds | `AGENTSDK-REQ-020` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > shutdown :: logs shutdown message by default` | Vitest sync | `AGENTSDK-REQ-021` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > shutdown :: calls custom shutdown handler instead of logging` | Vitest sync | `AGENTSDK-REQ-021` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > shutdown :: is idempotent when called multiple times` | Vitest sync; two calls | `AGENTSDK-REQ-021` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > middleware :: returns a function` | Vitest sync | `AGENTSDK-REQ-022` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > middleware :: calls next middleware` | Vitest async; real timers | `AGENTSDK-REQ-022` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > middleware :: calls a response callback for every message` | Vitest async; two invocations | `AGENTSDK-REQ-022` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > middleware :: calls critical response callback when duration exceeds threshold` | Vitest async; 150 ms versus 100 ms | `AGENTSDK-REQ-022` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > middleware :: does not call critical response callback when duration is below threshold` | Vitest async; fast path | `AGENTSDK-REQ-022` |
| `sdks/js/agent-sdk/src/middleware/PerformanceMonitor.test.ts` | `PerformanceMonitor > middleware :: uses default critical response handler when not provided` | Vitest async; 150 ms; console.warn | `AGENTSDK-REQ-022` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > static helpers :: builds a session key` | Vitest sync | `AGENTSDK-REQ-023` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > static helpers :: builds a step key` | Vitest sync | `AGENTSDK-REQ-023` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > Builder API :: returns this from all methods for chaining` | Vitest sync; select, text, complete, and cancel | `AGENTSDK-REQ-023` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > select step :: sends actions when the trigger command is received` | Vitest async integration; /setup; waitFor and sync | `AGENTSDK-REQ-024` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > select step :: records the answer and completes when an intent is received` | Vitest async; setup:color to blue | `AGENTSDK-REQ-024` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > text step :: sends the description as text when the trigger command is received` | Vitest async; /config | `AGENTSDK-REQ-024` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > text step :: can send the description as markdown` | Vitest async; Markdown flag | `AGENTSDK-REQ-024` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > text step :: records the answer and completes when a text reply is received` | Vitest async; answer Alice | `AGENTSDK-REQ-024` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > multi-step wizard :: advances through all steps and calls complete with all answers` | Vitest async; select pro then email | `AGENTSDK-REQ-025` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > cancel :: adds a cancel button to select steps and handles cancel intent` | Vitest async; generated cancel ID and default label | `AGENTSDK-REQ-025` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > cancel :: uses a custom cancel label` | Vitest async; Abort | `AGENTSDK-REQ-025` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > restart :: cancels existing session and restarts when command is sent again` | Vitest async; repeated /setup | `AGENTSDK-REQ-025` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > DM mode :: creates a DM conversation and sends steps there` | Vitest async; group trigger and text in DM | `AGENTSDK-REQ-026` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > DM mode :: completes a text step wizard via DM when triggered from a group` | Vitest async; API-key answer | `AGENTSDK-REQ-026` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > DM mode :: completes a multi-step wizard entirely via DM` | Vitest async; select and text | `AGENTSDK-REQ-026` |
| `sdks/js/agent-sdk/src/middleware/ActionWizard.test.ts` | `ActionWizard > session isolation :: maintains separate sessions for different senders` | Vitest async; two DMs and one reply | `AGENTSDK-REQ-026` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > types :: types the message content as string in command handlers` | Vitest sync compile-time assertion | `AGENTSDK-REQ-027` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > command arguments :: should pass only arguments to the handler, not the command itself` | Vitest async; /tx 0.1 | `AGENTSDK-REQ-027` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > command arguments :: should pass empty string for commands without arguments` | Vitest async; /balance | `AGENTSDK-REQ-027` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > command arguments :: should preserve multiple arguments with spaces` | Vitest async; /send 5 USDC to Alix | `AGENTSDK-REQ-027` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > commandList :: returns an empty array when no commands are registered` | Vitest sync | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > commandList :: returns commands in lowercase as they are stored` | Vitest sync; two mixed-case commands | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > commandList :: does not include the default handler in the command list` | Vitest sync | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > command descriptions :: should accept a description as the second parameter` | Vitest sync | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > command descriptions :: should work without a description (backwards compatible)` | Vitest sync | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > command descriptions :: should throw an error when description is provided but handler is missing` | Vitest sync negative | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > helpCommand config :: should auto-register a help command when helpCommand is provided` | Vitest sync | `AGENTSDK-REQ-028` |
| `sdks/js/agent-sdk/src/middleware/CommandRouter.test.ts` | `CommandRouter > helpCommand config :: should not register help command when helpCommand is not provided` | Vitest sync | `AGENTSDK-REQ-028` |

Runner requirements: Node 22 or later; `yarn test` runs `tsc --noEmit` and `vitest run --typecheck`; Vitest uses the Node environment, a 120-second test timeout, and a 60-second hook timeout. Integration cases require local xmtpd. Reconnect cases also require Toxiproxy. Global teardown removes SQLite files. No source declaration uses `.each`, skip, todo, or only.
