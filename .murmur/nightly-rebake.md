---
workspace: dev
service_profile: murmur-default-service-profile
persona: murmur-programmer
out: report
idle_timeout: 90m
on_idle: terminate
max_concurrent: 1
tags:
  - nightly-rebake
on:
  schedule:
    cron: "0 2 * * 1-5"
    timezone: America/Los_Angeles
---

# Nightly libxmtp Image Rebake

Rebake the `libxmtp` recipe so the `dev` workspace image stays fresh against
the tip of the `self-hosted` branch, then roll the workspace forward.

The image's value is its pre-warmed Nix store: the `default` and `rust` dev
shells and the backend musl container image are built in at bake time. That
warm store is pinned to `flake.lock` and `Cargo.lock` as they were when the
bake ran. When those files move, agents pay to rebuild whatever changed. This
job exists to keep that gap small.

Run every step with the `murmur` CLI. Report what you did in plain prose.

## Step 1: Check whether a rebake is warranted

Read the current workspace image and find the commit the last bake used:

    murmur get workspace dev
    murmur get image <imageRef>

Compare the tip of `self-hosted` against the last bake. The bake clones the
branch itself, so the comparison that matters is whether the branch moved:

    git ls-remote https://github.com/xmtp/libxmtp refs/heads/self-hosted

Record the commit the current image was built from. If you cannot determine
it, treat the rebake as warranted and say so.

**Skip the rebake** when the branch has not moved since the last successful
bake AND that bake completed within the last 24 hours. In that case stop here
and report "no rebake needed", naming the commit. A wasted 50-minute bake on
an unchanged branch costs real money.

Otherwise continue.

## Step 2: Confirm you can actually bake

You run as a service profile, not as a developer. Probe your authority before
doing anything else. `--resource` must come before the permission:

    murmur check-permissions bake.create environment.list placement-sa.assume
    murmur check-permissions --resource workspace/dev workspace.edit

If `bake.create`, `environment.list`, or `placement-sa.assume` is denied, STOP.
Do not attempt workarounds and do not spawn a child under another profile.
Report exactly which permission was denied and that an administrator must grant
it to `murmur-default-service-profile`. This is the expected failure on the
first run if the profile was never granted bake authority.

If `workspace.edit` is denied but the bake permissions are granted, you may
still bake; you simply cannot roll the workspace forward in step 4. Say so.

## Step 3: Bake

The recipe content does not change between runs, so the image cache would
return the existing image. `--force-new` is REQUIRED to actually rebuild
against the current branch tip:

    murmur bake libxmtp \
      --environment libxmtp-aws-large \
      --placement murmur-aws-us-west-2 \
      --service-profile murmur-default-service-profile \
      --service-account arn:aws:iam::992345333949:instance-profile/murmur-vm \
      --force-new

The command returns immediately. Note that several runs can share one hash
(the hash covers recipe content, which does not change between nightly runs),
so identify THIS run by its RUN ID, not by the hash alone.

Capture the full hash and run id of the new running row:

    murmur bakes ls --all=false --full-hash

Track that hash, never the recipe name — `murmur bakes ls` lists historical
bakes too, so matching on the recipe name would match a previous run and you
would report on the wrong bake.

Poll until the bake leaves the running list. A full bake takes 30-55 minutes:
it installs Nix and Docker, warms both dev shell closures, cross-compiles the
backend for musl, and pre-pulls the stack's Docker images.

The provisioning script budgets its own time against the 1h ceiling and prints
a timing line for every stage, like `=== [12m] done: rust dev shell (7m) ===`.
Read those lines to see where a slow bake spent its time. If more than 35
minutes have elapsed when the backend image step is reached, the script
deliberately SKIPS it and says so. That is a healthy bake, not a failure: the
image is still correct and agents build the backend on their first
`just backend up`.

Do not conclude the bake has hung before 60 minutes. The recipe's timeout is 1h,
which is the platform maximum, so a bake that exceeds it fails and produces no
image.

    while murmur bakes ls --all=false --full-hash | grep -q "$HASH"; do
      sleep 60
    done
    murmur bakes ls --full-hash | grep "$HASH"

A row whose phase is `completed` or `cached` succeeded. Anything else failed.

If the bake TIMED OUT, say so explicitly in your report, and quote the stage
timing lines so the slow stage is identified. The warm-stage budget may need
tightening — the knobs are the 35-minute guard before the backend image step
and the per-step `timeout` values in the recipe's provisioning script. Do not
change them yourself; report and stop.

## Step 4: Roll the workspace forward

Only on a successful bake. Find the new image name, which follows the pattern
`libxmtp-<first 16 hex of hash>-us-west-2`:

    murmur get image | grep libxmtp

Point the workspace at it. Use `patch`, never `set`: `set` is a full replace
and would silently drop the workspace's repos, `minIdle`, placement, and
service account.

    murmur patch workspace dev --set image_ref=<new-image-name>

Then verify by reading the workspace back and confirming `imageRef` changed
and everything else is intact.

## Step 5: Report

State plainly:

- whether you rebaked or skipped, and why
- the commit the new image was built from
- how long the bake took, and the per-stage timing lines
- whether the backend image warm ran or was skipped
- whether you rolled the workspace forward, and the new image name
- any permission that was denied

If the bake FAILED, report the error from the bakes table and do NOT patch the
workspace. Leaving the workspace on the last known-good image is correct — a
broken image would break every agent spawn until someone noticed.

Do not modify the recipe. Do not open pull requests. If something looks wrong
beyond a failed bake, report it rather than trying to fix it.
