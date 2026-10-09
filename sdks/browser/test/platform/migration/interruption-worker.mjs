// Pause the private file close. The test terminates this worker before publication.
const pending = [];
self.onmessage = (event) => pending.push(event);
const open = FileSystemFileHandle.prototype.createSyncAccessHandle;
FileSystemFileHandle.prototype.createSyncAccessHandle = async function (
  ...args
) {
  const handle = await open.apply(this, args);
  if (this.name.startsWith("archive-")) {
    const close = handle.close.bind(handle);
    handle.close = function () {
      if (
        new URL(self.location.href).searchParams.get("phase") === "after-close"
      )
        close();
      console.log("MIGRATION_PUBLICATION_PAUSED");
      const deadline = Date.now() + 60000;
      while (Date.now() < deadline) {
        /* The page must terminate this worker. */
      }
      throw new Error("The test did not terminate the paused worker");
    };
  }
  return handle;
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
