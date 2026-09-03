# Existing behavior

This folder documents how the systems that the self-hosted backend replaces behave today. Each page was written from the source code with file and function citations, reviewed adversarially against the source repository, and corrected in place. Every page ends with a `## Review status` section that lists the findings applied, the findings rejected with evidence, and the residual risk.

These pages are ephemeral. They are deleted with the rest of `docs/self-hosted` when the project ends.

| Page | Covers |
| --- | --- |
| [xmtp-node-go.md](xmtp-node-go.md) | The v3 backend: MLS API and Identity API methods, schema and queries, advisory-lock ordering, rate limiting, IP authorization, the XIP-83 `Subscribe` implementation, pruning, config, metrics |
| [xmtpd.md](xmtpd.md) | The v4 backend: procedures, envelope model, topic bytes, migrations and partitioning, identity flow, subscriptions, rate limiting, authentication, pruning, config, limits |
| [xip-83.md](xip-83.md) | The XIP-83 bidirectional subscription protocol: server and client requirements, test cases, conformance matrix against xmtpd |
| [proto.md](proto.md) | The `proto` repository: file inventory with use-by-libxmtp classification, verbatim definitions of the `backend.proto` dependency closure and the v3 and v4 services, build tooling |
| [mls-validation-service.md](mls-validation-service.md) | The MLS validation service: RPCs with ordered validation steps, association state machine and error table, smart contract wallet verification, callers, packaging, tests |
| [libxmtp-api-callers.md](libxmtp-api-callers.md) | The libxmtp client's backend calls: trait surface, per-operation caller catalog, topic model, cursor invariants, configuration constants, requirements, gaps against `backend.proto` |
| [libxmtp-streaming.md](libxmtp-streaming.md) | The libxmtp streaming stack: transport, the XIP-83 client, the subscriptions module, binding and SDK stream APIs, requirements, simplifications |

Facts that cut across pages and matter before the backend API spec is written:

- Neither backend orders across topics. `xmtp-node-go` orders within a topic. `xmtpd` orders per originator.
- `xmtp-node-go` dedupes publishes by content hash and treats a repeat as success. `xmtpd` stores duplicates.
- Both backends parse group messages only. Neither can check group membership.
- `xmtpd` rewrites error text through an interceptor for every status except `INVALID_ARGUMENT`, `UNIMPLEMENTED`, and `NOT_FOUND`.
- Group ids are 16 bytes. Installation keys and inbox ids are 32 bytes.
