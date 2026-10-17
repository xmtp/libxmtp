# Fly development deployment

Use `dev/fly/deploy-backend IMAGE` and `dev/fly/deploy-observability`.
These commands change live Fly resources.

Run `node dev/fly/render-dashboard.mjs` before building the Grafana image.
The generated dashboard is ignored. Its source is the shared Docker dashboard.
Run `node --test dev/fly/render-dashboard.test.mjs` after dashboard changes.

Validate workflow changes with `dev/nix-shell 'just lint-config'` and
`dev/nix-shell 'just test dev'`, which also runs the dashboard test.
Validate each Fly configuration with `fly config validate --strict` from its
directory. Private apps must not have public services or public IP addresses.

Keep all services in `sjc`. Preserve the server identifier, database, and volumes.
Use Fly secrets for credentials. Do not print secret values in deployment logs.
