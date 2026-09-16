import { readFileSync, writeFileSync } from "node:fs";

// Keep the shared dashboard as the source of truth. Restrict Fly queries to
// this environment so other apps in the organization cannot mix into panels.
const source = new URL("../docker/grafana/dashboards/backend.json", import.meta.url);
const target = new URL("grafana/backend.json", import.meta.url);
const dashboard = JSON.parse(readFileSync(source, "utf8"));

function adapt(value) {
  if (Array.isArray(value)) return value.map(adapt);
  if (value === null || typeof value !== "object") return value;
  const result = Object.fromEntries(
    Object.entries(value).map(([key, item]) => [key, adapt(item)]),
  );
  if (result.targets?.some((target) => /\btraces_/.test(target.expr ?? ""))) {
    result.datasource = { type: "prometheus", uid: "trace-metrics" };
  }
  if (typeof result.expr === "string" && /\btraces_/.test(result.expr)) {
    result.datasource = { type: "prometheus", uid: "trace-metrics" };
  }
  if (typeof result.expr === "string") {
    result.expr = result.expr.replace(
      /\b((?:xmtp_|grpc_)[a-zA-Z0-9_]+)(\{[^}]*\})?/g,
      (_, metric, labels) =>
        `${metric}{app="xmtp-backend-dev"${labels && labels !== "{}" ? `,${labels.slice(1, -1)}` : ""}}`,
    );
    // Fly does not expose the local Compose job label.
    result.expr = result.expr.replace(
      'up{job="backend"}',
      'max(present_over_time(xmtp_backend_info{app="xmtp-backend-dev"}[1m])) or vector(0)',
    );
  }
  return result;
}

const rendered = adapt(dashboard);
rendered.title = "XMTP Backend — Fly development";
rendered.uid = "xmtp-backend-dev";
writeFileSync(target, `${JSON.stringify(rendered, null, 2)}\n`);
