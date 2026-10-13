// Observe the real OPFS archive read without replacing it.
const pending = [];
self.onmessage = (event) => pending.push(event);
const read = Blob.prototype.arrayBuffer;
Blob.prototype.arrayBuffer = function (...args) {
  if (this instanceof File && this.name.startsWith("archive-"))
    console.log("MIGRATION_ARCHIVE_READ " + this.size);
  return read.apply(this, args);
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
