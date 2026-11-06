// Observe real access handles before the SDK can terminate its worker.
const pending = [];
self.onmessage = (event) => pending.push(event);
const phase = new URL(import.meta.url).searchParams.get("phase");
const root = await navigator.storage.getDirectory();
const opened = { source: 0, working: 0 };
const closed = { source: 0, working: 0 };
let privateAttempts = 0;
let faultCount = 0;
let deleteFaults = 0;
let deletingFamily = false;
self.addEventListener("message", ({ data }) => {
  if (phase === "delete-family" && data.key === "StorageAdmin.deleteFile")
    deletingFamily = true;
});
const open = FileSystemFileHandle.prototype.createSyncAccessHandle;
FileSystemFileHandle.prototype.createSyncAccessHandle = async function (
  ...args
) {
  const path = await root.resolve(this);
  const kind = path?.includes(".xmtp-migration-working") ? "working" : "source";
  if (kind === "working" && ++privateAttempts === 8 && phase === "clear-all") {
    faultCount++;
    throw new DOMException(
      "Injected clear_all partial acquisition",
      "UnknownError",
    );
  }
  const handle = await open.apply(this, args);
  opened[kind]++;
  let copied = false;
  const write = handle.write.bind(handle);
  handle.write = (bytes, options) => {
    const count = write(bytes, options);
    if (kind === "working" && bytes.byteLength > 4096) copied = true;
    return count;
  };
  const read = handle.read.bind(handle);
  handle.read = (bytes, options) => {
    if (phase === "delete" && copied && bytes.byteLength <= 4096) {
      faultCount++;
      throw new DOMException(
        "Injected private SQLite open failure",
        "UnknownError",
      );
    }
    return read(bytes, options);
  };
  const truncate = handle.truncate.bind(handle);
  handle.truncate = (size) => {
    if (deletingFamily && size === 4096 && ++deleteFaults === 2) {
      faultCount++;
      throw new DOMException(
        "Injected family deletion failure",
        "UnknownError",
      );
    }
    if (phase === "delete" && copied && size === 4096) {
      deleteFaults++;
      throw new DOMException(
        "Injected private deletion failure",
        "UnknownError",
      );
    }
    return truncate(size);
  };
  const close = handle.close.bind(handle);
  handle.close = () => {
    closed[kind]++;
    return close();
  };
  return handle;
};
const post = self.postMessage.bind(self);
self.postMessage = (message, ...args) => {
  if (["return", "error"].includes(message.t)) {
    console.log(
      "MIGRATION_HANDLES " +
        JSON.stringify({
          phase,
          opened,
          closed,
          privateAttempts,
          faultCount,
          deleteFaults,
        }),
    );
  }
  return post(message, ...args);
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
