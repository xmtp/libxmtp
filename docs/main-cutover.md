# Main cutover

Use this runbook to move the accepted self-hosted source and website to main and
xmtp.org. Preparation does not change live DNS, Pages, or backend configuration.
The primary branch remains self-hosted until the activation change merges.

## Scope and owner

The sole maintainer owns the accepted SHA, branch update, website routing, and
service checks. Use the existing automatic deployment workflows. No package release is planned.
The existing release automation is unchanged and remains separate work.

## Prepare while feature PRs continue

1. Review and merge C and D into self-hosted. Keep activation E draft.
2. Prepare apex ownership verification, current URL inventory, old-host redirect
   rules, certificate handover, and rollback records.
3. Keep new feature work on self-hosted. Record the final inclusion decision for
   each remaining PR. An old CI result does not approve a later source.
4. Leave release/8.0.0 and package publication outside this cutover. Review them
   separately when a release is planned.

## Freeze and accept the source

1. Choose a short source freeze. Finish any active deployment before the branch
   move, and record which normal main-push jobs will run.
2. Stop all merges and bypass writes to main and self-hosted. Save both refs,
   website artifacts, DNS records, deployment IDs, image
   digests, and database backup/restore records.
3. Refresh and merge activation E. Record the exact accepted self-hosted SHA.
4. Run full CI on that SHA. Check child results, not only Test/Lint. Merge runs
   include iOS, Android, and Android Messenger suites. Manual recovery and
   Windows/platform checks still need their own receipts for release claims.
5. Build the current site and references. Run the two required docs checks and
   the staged origin check. Never create or copy a build stamp by hand.

```sh
 dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs check'
 dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs check-external'
 dev/nix-shell 'just docs check-cutover https://xmtp.org'
```

The skip flag is the local PR setup only. The cutover acceptance record needs
Kotlin and Swift references from the full push build as well. The cutover check
validates the artifact; it does not prove DNS, HTTPS, or live HTTP redirects.

## Fast-forward and deploy

Confirm that main is still an ancestor and both refs equal the freeze record.
Use the existing approved ruleset bypass for a normal fast-forward push. Do not
force push, cherry-pick, squash, or rebase the existing self-hosted history.
Confirm that main equals the accepted SHA and the promoted commit list matches.

Before the fast-forward, finish DNS verification, redirect staging, TLS
handover, and backend backup/migration checks. The first main push can start
docs, chat, backend, and image publication through their normal triggers.

Keep main unchanged while those jobs finish. Check each workflow checkout and
deployed image or artifact against the accepted SHA. Use the existing workflow
controls for a failed deployment.

## Website routing and rollback

Set the actual Pages repository custom domain, then switch accepted apex/www
routing. Verify the certificate and enforce HTTPS. Keep both old docs hosts on
an explicit redirect service with valid TLS. Preserve paths and queries and
map changed paths. Preserve /signatures and /signatures/ without changing the
identity signing text. Remove any old DNS pointer to an unbound Pages domain.

The static meta-refresh pages are local path fallbacks. They are not old-host
HTTP redirects. Check canonical/social/sitemap URLs, LLM exports, search,
reference assets, deep redirects, and IPv4/IPv6 HTTPS before acceptance.

If routing or TLS fails, restore the saved site and DNS/origin records. Keep
old site artifacts for at least seven days. For service rollback, use an accepted
immutable image or deployment only when its database schema is compatible.
Restore database backups into a separate database and validate before switching.
Do not overwrite an immutable package or reassign an existing release version.

## Later package and user acceptance

Package publication and user migration are separate from this branch/site move.
When a release is planned, use clean consumers for Node, Browser, Agent, SwiftPM, and Maven. Record source,
workflow, version, checksum/digest, and run IDs. Node and Agent must use matching
versions. Test browser worker/WASM resolution, OPFS lifecycle, mobile native
slices, and all supported Node platforms. CocoaPods trunk is not published by
the current iOS workflow. React Native is not supported by this transition.

Test history/recovery page cursors, deleted boundaries, conversion skips, and
continuation through a short or empty page. Existing workspace tests are not
proof of the installed packages.

Legacy conversion uses the normal SDK packages and a closed source database.
It preserves the source and committed WAL data. A record above the 64 MiB
budget fails the conversion. Browser read/import accepts an archive at most
100,000,000 bytes and runs in memory; native file import has no Browser read
limit. Stored expiry deadlines exclude messages; null deadlines can include
history whose original disappearing status is unknown.

The archive has no owner-inbox check, installation keys, MLS secrets, or
attachment files. Select the correct destination inbox. Imported history does
not activate future live traffic. Conversation recreation remains unfinished
in the guide; do not advertise complete live migration without that work.

Test two users/two installations, public gRPC/gRPC-Web ingress, providers,
restart and durable recovery, backup restore, and delivered alerts. The public
unauthenticated Fly development endpoint and TCP readiness are not production
acceptance. Observe each accepted cutover for 24 hours before closing it.
