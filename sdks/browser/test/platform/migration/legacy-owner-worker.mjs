// Model a legacy SDK that owns OPFS handles without the current SDK Web Lock.
const handles = [];
async function hold(directory) {
  for await (const entry of directory.values()) {
    if (entry.kind === "directory") await hold(entry);
    else handles.push(await entry.createSyncAccessHandle());
  }
}
self.onmessage = async ({ data }) => {
  try {
    const root = await navigator.storage.getDirectory();
    await hold(await root.getDirectoryHandle(data.directory));
    self.postMessage({ count: handles.length });
  } catch (error) {
    self.postMessage({ error: String(error) });
  }
};
