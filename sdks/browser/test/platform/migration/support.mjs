export function check(value, message) {
  if (!value) throw new Error(message);
}

export async function fixtureCall(data) {
  const worker = new Worker(new URL("./fixture-worker.mjs", location.href), {
    type: "module",
  });
  try {
    return await new Promise((resolve, reject) => {
      worker.onmessage = ({ data }) =>
        data.ok ? resolve(data.value) : reject(new Error(data.error));
      worker.onerror = (error) => reject(new Error(error.message));
      worker.postMessage(data);
    });
  } finally {
    worker.terminate();
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

export function exportSourceBytes(path) {
  return fixtureCall({ operation: "export", path });
}

export async function assertSourceUnchanged(before, path) {
  const after = await exportSourceBytes(path);
  check(
    before.length === after.length &&
      before.every((byte, index) => byte === after[index]),
    "source bytes changed",
  );
}

export async function assertFailedPreparation(sdk, args, name) {
  const baseline = new Uint8Array(
    await (
      await fetch("/crates/xmtp_legacy_migration/fixtures/stable.db3")
    ).arrayBuffer(),
  );
  await fixtureCall({
    operation: "import",
    path: "/small.db3",
    bytes: baseline,
  });
  await sdk.prepareMigrationArchive({ ...args, databasePath: "/small.db3" });
  const completed = await sdk.readMigrationArchive(args.outputPath);
  for (const previous of [false, true]) {
    const outputPath = previous ? args.outputPath : "new-row-size.xmtp";
    let failure;
    try {
      await sdk.prepareMigrationArchive({ ...args, outputPath });
    } catch (error) {
      failure = error;
    }
    check(
      failure instanceof sdk.XmtpError.MigrationRecordRead,
      name +
        ": oversized row did not return MigrationRecordRead (" +
        (failure ? String(failure) : "preparation succeeded") +
        ")",
    );
    // Check private files before an archive read can remove them.
    const root = await navigator.storage.getDirectory();
    const archives = await root.getDirectoryHandle("xmtp-migration-archives");
    const folder = await archives.getDirectoryHandle(
      encodeURIComponent(outputPath),
    );
    check(
      (await Array.fromAsync(folder.keys())).length === (previous ? 1 : 0),
      "private output remains after failure",
    );
    if (previous) {
      const preserved = await sdk.readMigrationArchive(outputPath);
      check(
        preserved.length === completed.length &&
          preserved.every((byte, index) => byte === completed[index]),
        "completed output changed",
      );
    } else {
      let absent = false;
      try {
        await sdk.readMigrationArchive(outputPath);
      } catch (error) {
        absent = error instanceof sdk.XmtpError.MigrationOutput;
      }
      check(absent, "failed preparation published a new output");
    }
  }
  const retry = await sdk.prepareMigrationArchive({
    ...args,
    databasePath: "/small.db3",
  });
  check(retry.messageCount === 3n, "failure did not release storage for retry");
}
