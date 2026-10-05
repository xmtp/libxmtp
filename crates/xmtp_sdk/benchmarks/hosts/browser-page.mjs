import * as accounts from "@bench/accounts";
import * as pure from "@bench/pure";
import * as sdk from "@bench/sdk";

import { publicApi } from "./sdk.mjs";
import { seed, measure } from "./workload.mjs";

window.benchmark = async (request, fixture, state, backend) => {
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
    fixture,
    state,
    request.workload,
    `${prefix}-${request.workload}.db`,
  );
  // Deliver observer records for the completed timed work before teardown.
  await new Promise((resolve) => setTimeout(resolve, 0));
  for (const entry of observer.takeRecords()) tasks.push(entry);
  observer.disconnect();
  return {
    ...result,
    long_tasks_ms: tasks
      .map(
        (entry) =>
          Math.min(
            entry.startTime + entry.duration,
            result.timing_window.end_ms,
          ) - Math.max(entry.startTime, result.timing_window.start_ms),
      )
      .filter((duration) => duration > 50),
    source: {
      fixture_sha256: request.fixture_sha256,
      package_sha256: request.package_sha256,
    },
  };
};
