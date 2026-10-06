// Pause a real output stream. The test terminates this page before publication.
const pending = [];
self.onmessage = (event) => pending.push(event);
const close = FileSystemWritableFileStream.prototype.close;
FileSystemWritableFileStream.prototype.close = async function () {
  if (new URL(self.location.href).searchParams.get("phase") === "after-close")
    await close.call(this);
  console.log("MIGRATION_PUBLICATION_PAUSED");
  await new Promise(() => {});
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
