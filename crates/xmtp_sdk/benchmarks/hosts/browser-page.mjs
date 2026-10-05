import * as accounts from "@bench/accounts";
import * as pure from "@bench/pure";
import * as sdk from "@bench/sdk";

import { publicApi } from "./sdk.mjs";
import { seed, measure } from "./workload.mjs";

// Main-thread tasks above this length count as long tasks.
const LONG_TASK_MS = 50;

window.benchmark = async (request, fixture, state, backend) => {
  // Load the pure codecs before any timer starts.
  await pure.initPureWasm();
  const api = publicApi(sdk, pure, "browser", backend, accounts);
  const prefix = `${request.sample ?? "setup"}`;
  if (
    request.phase === "setup" ||
    (request.phase === "reset" && request.workload === "stream")
  ) {
    const seeded = await seed(
      api,
      fixture,
      { sender: `${prefix}-sender.db`, receiver: `${prefix}-receiver.db` },
      request.phase === "reset",
    );
    return { ready: true, state: seeded };
  }
  if (request.phase === "reset") return { ready: true };
  const tasks = [];
  const observer = new PerformanceObserver((entries) => {
    for (const entry of entries.getEntries()) tasks.push(entry);
  });
  observer.observe({ type: "longtask", buffered: false });
  const result = await measure(
    api,
    state,
    request.workload,
    `${prefix}-${request.workload}.db`,
  );
  // Deliver observer records for the completed timed work before teardown.
  await new Promise((resolve) => setTimeout(resolve, 0));
  for (const entry of observer.takeRecords()) tasks.push(entry);
  observer.disconnect();
  // Count only the part of each task inside the timed window.
  const { start_ms: start, end_ms: end } = result.timing_window;
  return {
    ...result,
    long_tasks_ms: tasks
      .map(
        (entry) =>
          Math.min(entry.startTime + entry.duration, end) -
          Math.max(entry.startTime, start),
      )
      .filter((duration) => duration > LONG_TASK_MS),
  };
};
