# Legacy specs

Source material for the specs in `docs/specs/`: the six numbered documents this
project supersedes, and verbatim copies of the XIPs the specs draw on.

Everything here is **informational and non-authoritative**. Do not cite one as a
contract, do not add a requirement to one, and do not treat a `SEC-`, `ARC-`,
`STR-`, or `CFG-` identifier in code as a live reference. An approved spec in
`docs/specs/` wins wherever the two disagree, and an XIP has no authority of its
own (SPEC-008). This folder is deleted once every obligation in it has a
disposition (SPEC-013).

One exception, stated by SPEC-011: an obligation whose replacement has not been
approved yet still binds, and a reviewer treats it as authoritative for that
obligation alone. `docs/specs/PREFIXES.md` says which prefix each numbered
document owns and which specs replace it.

The XIP copies are refreshed from
[xmtp/XIPs](https://github.com/xmtp/XIPs/tree/main/XIPs); do not edit them
otherwise. XIP-68, which the plan names as the source for `FORK`, is not
published there, so the commit log's source material is the code under
`crates/xmtp_mls/src/groups/commit_log.rs`.
