// Fail a real OPFS write after it writes bytes to the private output.
const pending = [];
self.onmessage = (event) => pending.push(event);
const open = FileSystemFileHandle.prototype.createSyncAccessHandle;
FileSystemFileHandle.prototype.createSyncAccessHandle = async function (
  ...args
) {
  const handle = await open.apply(this, args);
  if (this.name.startsWith("archive-")) {
    const write = handle.write.bind(handle);
    handle.write = function (bytes, options) {
      write(bytes, options);
      throw new DOMException(
        "Injected storage quota failure",
        "QuotaExceededError",
      );
    };
  }
  return handle;
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
