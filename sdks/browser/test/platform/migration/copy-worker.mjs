// Observe actual VFS reads and stop a real private-file copy when requested.
const pending = [];
self.onmessage = (event) => pending.push(event);
const phase = new URL(import.meta.url).searchParams.get("phase") ?? "measure";
let maximum = 0;
let total = 0;
let fired = false;
const open = FileSystemFileHandle.prototype.createSyncAccessHandle;
FileSystemFileHandle.prototype.createSyncAccessHandle = async function (
  ...args
) {
  const handle = await open.apply(this, args);
  const read = handle.read.bind(handle);
  handle.read = function (bytes, options) {
    maximum = Math.max(maximum, bytes.byteLength);
    total += bytes.byteLength;
    return read(bytes, options);
  };
  const write = handle.write.bind(handle);
  handle.write = function (bytes, options) {
    const count = write(bytes, options);
    if (!fired && bytes.byteLength === 64 * 1024 && phase !== "measure") {
      fired = true;
      if (phase === "quota")
        throw new DOMException(
          "Injected working-copy quota",
          "QuotaExceededError",
        );
      console.log("MIGRATION_COPY_PAUSED");
      const deadline = Date.now() + 60000;
      while (Date.now() < deadline) {}
    }
    return count;
  };
  const close = handle.close.bind(handle);
  handle.close = function (...args) {
    console.log("MIGRATION_SOURCE_IO " + JSON.stringify({ total, maximum }));
    return close(...args);
  };
  return handle;
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
