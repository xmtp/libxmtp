import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

execFileSync(process.execPath, [new URL("render-dashboard.mjs", import.meta.url).pathname]);
const source = JSON.parse(readFileSync(new URL("../docker/grafana/dashboards/backend.json", import.meta.url)));
const rendered = JSON.parse(readFileSync(new URL("grafana/backend.json", import.meta.url)));

test("the Fly dashboard preserves every shared panel", () => {
  assert.deepEqual(rendered.panels.map((panel) => panel.title), source.panels.map((panel) => panel.title));
  assert.equal(rendered.uid, "xmtp-backend-dev");
});

test("trace metrics use private Prometheus without a Fly app label", () => {
  const panels = rendered.panels.filter((panel) => panel.targets?.some((target) => /\btraces_/.test(target.expr ?? "")));
  assert.equal(panels.length, 2);
  for (const panel of panels) {
    assert.equal(panel.datasource.uid, "trace-metrics");
    for (const target of panel.targets) {
      assert.equal(target.datasource.uid, "trace-metrics");
      assert.doesNotMatch(target.expr, /app=/);
    }
  }
});

test("backend queries are restricted to the development app", () => {
  for (const panel of rendered.panels) {
    for (const target of panel.targets ?? []) {
      for (const match of (target.expr ?? "").matchAll(/\b(?:xmtp_|grpc_)[a-zA-Z0-9_]+(\{[^}]*\})?/g)) {
        assert.match(match[1], /app="xmtp-backend-dev"/);
      }
      assert.doesNotMatch(target.expr ?? "", /job="backend"/);
    }
  }
});
