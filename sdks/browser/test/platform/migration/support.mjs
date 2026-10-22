export function check(value, message) {
  if (!value) throw new Error(message);
}

export async function fixtureCall(data) {
  const lifetimeLock =
    data.lifetimeLock ?? "migration-fixture:" + crypto.randomUUID();
  const worker = new Worker(new URL("./fixture-worker.mjs", location.href), {
    type: "module",
  });
  try {
    return await new Promise((resolve, reject) => {
      worker.onmessage = ({ data }) =>
        data.ok ? resolve(data.value) : reject(new Error(data.error));
      worker.onerror = (error) => reject(new Error(error.message));
      worker.postMessage({ ...data, lifetimeLock });
    });
  } finally {
    worker.terminate();
    await navigator.locks.request(lifetimeLock, () => {});
  }
}

export function exportSourceBytes(path) {
  return fixtureCall({ operation: "export", path });
}

export async function assertSourceUnchanged(before, path) {
  const after = await exportSourceBytes(path);
  check(
    before.length === after.length &&
      before.every((byte, index) => byte === after[index]),
    "source bytes changed",
  );
}
