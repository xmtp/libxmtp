// Inject normal failures at private-file creation, writing, flush, or publication.
const pending = [];
self.onmessage = (event) => pending.push(event);
const fault = new URL(import.meta.url).searchParams.get("fault") ?? "write";
const fail = () => {
  throw new DOMException("Injected storage failure", "QuotaExceededError");
};
const open = FileSystemFileHandle.prototype.createSyncAccessHandle;
FileSystemFileHandle.prototype.createSyncAccessHandle = async function (
  ...args
) {
  if (this.name.startsWith("archive-") && fault === "open") fail();
  const handle = await open.apply(this, args);
  if (this.name.startsWith("archive-")) {
    if (fault === "write" || fault === "close") {
      const write = handle.write.bind(handle);
      handle.write = function (bytes, options) {
        write(bytes, options);
        fail();
      };
    }
    if (fault === "flush") handle.flush = fail;
    if (fault === "close" || fault === "commit-close") {
      const close = handle.close.bind(handle);
      let calls = 0;
      handle.close = function () {
        close();
        calls += 1;
        throw new DOMException(
          fault === "commit-close" && calls === 1
            ? "Injected commit close failure"
            : "Injected close failure",
          "UnknownError",
        );
      };
    }
  }
  return handle;
};
const put = IDBObjectStore.prototype.put;
IDBObjectStore.prototype.put = function (...args) {
  const request = put.apply(this, args);
  if (fault === "publish" && this.name === "published")
    this.transaction.abort();
  return request;
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
