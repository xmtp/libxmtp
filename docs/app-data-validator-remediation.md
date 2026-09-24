# App-data validator remediation runbook

What to do when a **fielded release ships a receive-side validation bug** on the
app-data (post-migration) path — a validator that wrongly rejects commits other
versions accept, or wrongly accepts commits it shouldn't. The goal is always the
same: convert divergence into **pause** (recoverable by upgrading) and never into
**fork** (permanent group split). No wire-format migration is required for any
scenario below.

## The lever: per-group protocol-version floor bumps

`MlsGroup::update_group_min_version` (and `update_group_min_version_to_match_self`)
writes the group's `MIN_SUPPORTED_PROTOCOL_VERSION` floor — the legacy
`GroupMutableMetadata` attribute pre-migration, the `0x800A` AppData component
post-migration. Receive-side monotonicity means a floor can only rise.

Any member processing a commit on a group whose **committed** floor exceeds its
own version pauses the group (`paused_for_version`): the cursor holds, all
subsequent commits are deferred, and after the app upgrades the sweep unpauses
the group and reprocesses everything with fixed code. Pausing never accepts or
rejects anything, so a wrongly-paused client always converges once upgraded.

Writing the floor is super-admin-gated on groups (both members on DMs).

## Scenario: version X validator wrongly REJECTS valid commits

Fielded X clients fork off groups whenever the triggering commit shape appears.

1. Fix the validator in version Y.
2. Bump `PROPOSALS_MIN_PROTOCOL_VERSION` to Y so newly-migrating groups get the
   Y floor from bootstrap.
3. For existing migrated groups: have (super-admin) clients on Y bump each
   group's floor to Y — e.g. wire `update_group_min_version_to_match_self` into
   a recovery flow. X members pause **at the floor-bump commit** (its cursor
   holds there), so they never process the bug-triggering commits mid-stream;
   after upgrading to Y they reprocess everything with the fixed validator.
4. X members who already rejected a commit before the floor bump landed are
   forked; recovery for them is the normal fork-recovery path (re-add /
   re-welcome), not this runbook.

## Scenario: version X validator wrongly ACCEPTS bad state

E.g. the pre-#3863 registry-poison bug: state that wedges or corrupts readers.

1. Fix acceptance in Y, plus a read-side tolerance for the already-fielded bad
   state where possible (the #3863 pattern: preserved-but-invisible registry
   entries — bad state degrades to "that component is unavailable", not "no
   commit validates").
2. Floor-bump affected groups to Y as above so X members stop extending the bad
   state and pause until they can run the tolerant readers.

## Why the floor is trustworthy

The pause guards read only **committed** floor state (the pre-commit dict or
legacy GMM), never same-commit proposals — a floor value only exists after its
super-admin policy check passed on every honest receiver. See the floor-bump
convention in `xmtp_mls_common::app_data::registry_table` for the send-side
rules protocol evolution must follow.

**Caveat — the floor-bump commit is not self-pausing.** The commit that *raises*
the floor is itself validated under the *old* floor: on migrated groups
`ValidatedCommit::from_staged_commit` reads the floor from the post-commit
dictionary overlay (which includes the bump commit's own staged `AppDataUpdate`),
and `ProtocolVersionTooLow` is checked only *after* the other commit validators
and policy evaluation. So an X client pauses on the *next* commit it sees under
the raised floor — not on the bump commit itself. That means the floor-bump
commit must be one every affected (X) version can still *accept*: if a bug in the
bump commit's shape makes an X validator reject it before the floor check runs, X
clients fork instead of pausing. Keep the floor-bump commit format-boring (a plain
`MIN_SUPPORTED_PROTOCOL_VERSION` write, no new-format payloads) — this is exactly
why the convention requires the bump to land in a strictly-earlier, boring commit.

## Sharp edges

- **Monotonic means permanent.** A floor of `999.0.0` written by a rogue or
  buggy super admin pauses the group for everyone forever (send-side clamp
  `min_version <= own pkg_version` makes accidents unlikely; a malicious super
  admin can equally destroy a group in more direct ways).
- **The registry must stay loadable** for the post-migration floor to be
  readable — the entry-validation + tolerant-loader pair (#3863) is what keeps
  this lever alive on groups carrying unexpected registry state.
- **Same-device downgrades** below a group's committed floor pause at the next
  commit (pause-before-parse guards, #3864); unknown-`IntentKind` rows written
  by the newer build are excluded from outbound queries rather than wedging
  them. Both recover on re-upgrade.
