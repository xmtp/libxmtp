// Fail a real OPFS write after it writes bytes to the temporary output.
const pending = [];
self.onmessage = (event) => pending.push(event);
const write = FileSystemWritableFileStream.prototype.write;
FileSystemWritableFileStream.prototype.write = async function (bytes) {
  await write.call(this, bytes);
  throw new DOMException(
    "Injected storage quota failure",
    "QuotaExceededError",
  );
};
await import("/target/migration-packages/browser/worker.js");
for (const event of pending) self.onmessage(event);
