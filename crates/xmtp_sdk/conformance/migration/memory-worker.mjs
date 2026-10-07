// Measure transfers at the real output sink, including the former whole-file sink.
const pending = [];
self.onmessage = (event) => pending.push(event);
function measured(write, close) {
  let total = 0;
  let maximum = 0;
  return {
    write(bytes, ...args) {
      total += bytes.byteLength;
      maximum = Math.max(maximum, bytes.byteLength);
      return write(bytes, ...args);
    },
    close(...args) {
      console.log(
        `MIGRATION_OUTPUT_MEMORY ${JSON.stringify({ total, maximum })}`,
      );
      return close(...args);
    },
  };
}
const open = FileSystemFileHandle.prototype.createSyncAccessHandle;
FileSystemFileHandle.prototype.createSyncAccessHandle = async function (
  ...args
) {
  const handle = await open.apply(this, args);
  if (this.name.startsWith("archive-")) {
    const methods = measured(
      handle.write.bind(handle),
      handle.close.bind(handle),
    );
    handle.write = methods.write;
    handle.close = methods.close;
  }
  return handle;
};
const oldOpen = FileSystemFileHandle.prototype.createWritable;
FileSystemFileHandle.prototype.createWritable = async function (...args) {
  const stream = await oldOpen.apply(this, args);
  const methods = measured(
    stream.write.bind(stream),
    stream.close.bind(stream),
  );
  stream.write = methods.write;
  stream.close = methods.close;
  return stream;
};
await import("/target/sdk-packages/browser/typescript-wasm/worker-entry.gen.js");
self.onmessage = null;
for (const event of pending)
  self.dispatchEvent(new MessageEvent("message", { data: event.data }));
