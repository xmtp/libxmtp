import init, {
  import_source,
  export_source,
  leave_hot_journal,
  inspect_archive,
} from "/target/migration-fixture-host/fixture_host.js";
self.onmessage = async ({ data }) => {
  try {
    await init();
    if (data.operation === "hot-journal") {
      await navigator.locks.request(data.lifetimeLock, async () => {
        await leave_hot_journal(data.path);
        self.postMessage({ ok: true });
        await new Promise(() => {});
      });
      return;
    }
    const value =
      data.operation === "import"
        ? await import_source(data.path, data.bytes)
        : data.operation === "export"
          ? await export_source(data.path)
          : data.operation === "sizes"
            ? await (
                await import("/target/migration-fixture-host/fixture_host.js")
              ).inspect_archive_sizes(data.bytes, data.key)
            : data.operation === "record-sizes"
              ? await (
                  await import("/target/migration-fixture-host/fixture_host.js")
                ).inspect_archive_record_sizes(data.bytes, data.key)
              : await inspect_archive(data.bytes, data.key);
    self.postMessage({ ok: true, value });
  } catch (error) {
    self.postMessage({ ok: false, error: String(error) });
  }
};
