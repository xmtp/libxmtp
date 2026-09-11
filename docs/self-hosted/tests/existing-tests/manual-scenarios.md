# Manual test scenarios

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified scenario | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| `TEST_SCENARIOS.md` | `1. Sending and receiving as installations are added` | Manual scenario; fresh seeds; staged A1, B1, A2, B2, and A3 registrations | `MANUAL-REQ-001` |
| `TEST_SCENARIOS.md` | `2. Enumerate installations` | Manual scenario; depends on scenario 1; expects A=3 and B=2 from A1 | `MANUAL-REQ-002` |
| `TEST_SCENARIOS.md` | `3. Sending and receiving with varying network connections` | Manual scenario; offline toggle and cold restart | `MANUAL-REQ-003` |

Scope exclusions: `dev/test/big_group.sh` and `big_group_chaos.sh` are assertion-free, long-running data generators; `dev/test/browser-sdk` is an external test runner; and `dev/test/diff-coverage` is a report generator without a threshold. CI workflows consume the release CLI but add no source test declarations.
