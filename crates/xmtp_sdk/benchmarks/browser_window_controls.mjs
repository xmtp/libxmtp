import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { SourceTextModule, SyntheticModule, createContext } from "node:vm";

// Run the exact host module with controlled observer records and a fixed window.
const url = new URL("./hosts/browser-page.mjs", import.meta.url);
const source = await readFile(url, "utf8");
const records = [];
const cases = [
  ["inside", 150, 80, [80]],
  ["start_clipped", 60, 100, [60]],
  ["end_clipped", 240, 100, [60]],
  ["both_clipped", 50, 300, [200]],
  ["before_window", 0, 80, []],
  ["after_window", 310, 80, []],
  ["start_overlap_49", 69, 80, []],
  ["start_overlap_50", 70, 80, []],
  ["start_overlap_51", 71, 80, [51]],
  ["end_overlap_49", 251, 80, []],
  ["end_overlap_50", 250, 80, []],
  ["end_overlap_51", 249, 80, [51]],
];
for (const channel of ["callback", "takeRecords"]) {
  for (const [name, startTime, duration, expected] of cases) {
    const entries = [{ startTime, duration }];
    let disconnected = false;
    const context = createContext({
      window: {},
      setTimeout: (done) => done(),
      PerformanceObserver: class {
        constructor(callback) {
          this.callback = callback;
        }
        observe(options) {
          assert.deepEqual(
            { ...options },
            { type: "longtask", buffered: false },
          );
          if (channel === "callback")
            this.callback({ getEntries: () => entries });
        }
        takeRecords() {
          return channel === "takeRecords" ? entries : [];
        }
        disconnect() {
          disconnected = true;
        }
      },
    });
    const result = {
      duration_ms: 200,
      timing_window: { start_ms: 100, end_ms: 300 },
    };
    const modules = {
      "./sdk.mjs": { publicApi: () => ({}) },
      "./workload.mjs": {
        seed: () => {
          throw new Error("Setup is outside this control");
        },
        measure: async () => result,
      },
      "@bench/accounts": {},
      "@bench/pure": {},
      "@bench/sdk": {},
    };
    const module = new SourceTextModule(source, {
      context,
      identifier: url.href,
    });
    await module.link((specifier) => {
      assert.ok(Object.hasOwn(modules, specifier), specifier);
      const exports = modules[specifier];
      return new SyntheticModule(
        Object.keys(exports),
        function () {
          for (const [key, value] of Object.entries(exports))
            this.setExport(key, value);
        },
        { context },
      );
    });
    await module.evaluate();
    const actual = await context.window.benchmark(
      { side: "new", phase: "measure", workload: "page" },
      {},
      {},
      "fixture",
    );
    const label = `browser long-task ${channel}/${name}`;
    assert.deepEqual(Array.from(actual.long_tasks_ms), expected, label);
    assert.deepEqual(actual.timing_window, result.timing_window, label);
    assert.equal(actual.duration_ms, 200, label);
    assert.equal(disconnected, true, label);
    records.push({
      channel,
      name,
      startTime,
      duration,
      expected,
      actual: Array.from(actual.long_tasks_ms),
    });
  }
}
console.log(
  JSON.stringify(
    {
      source: url.href,
      source_sha256: createHash("sha256").update(source).digest("hex"),
      records,
    },
    null,
    2,
  ),
);
