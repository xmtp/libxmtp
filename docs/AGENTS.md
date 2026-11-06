# Documentation

Before submitting changes in `docs/` or `apps/docs/`, run from the root:

```sh
dev/nix-shell 'just docs check'
dev/nix-shell 'just docs check-external'
```

Use current site/reference outputs; never fabricate or copy a build stamp.
Report failures and fix broken links. Fixture/dry runs are not full-site passes.
For building, matching CI artifacts, native-reference exclusions, and Lychee
scope, follow [site validation](../apps/docs/VALIDATION.md).
For spec changes, use the relevant skill linked in [specs/README.md](specs/README.md).
