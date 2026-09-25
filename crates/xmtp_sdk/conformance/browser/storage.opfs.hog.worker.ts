let access: FileSystemSyncAccessHandle | undefined;

self.addEventListener("message", async (event: MessageEvent<"hold" | "release">) => {
  try {
    if (event.data === "hold") {
      if (access) throw new Error("pool file is already held");
      const root = await navigator.storage.getDirectory();
      const pool = await root.getDirectoryHandle(".opfs-libxmtp-metadata");
      const opaque = await pool.getDirectoryHandle(".opaque");
      for await (const [, entry] of opaque.entries()) {
        if (entry.kind === "file") {
          access = await entry.createSyncAccessHandle();
          break;
        }
      }
      if (!access) throw new Error("the SQLite SAH pool has no file");
    } else {
      if (!access) throw new Error("no pool file is held");
      access.close();
      access = undefined;
    }
    self.postMessage({ ok: true });
  } catch (error) {
    self.postMessage({ ok: false, message: String(error) });
  }
});
