import init, {
  import_source,
  export_source,
  inspect_archive,
} from "../../../target/migration-fixture-host/fixture_host.js";
self.onmessage = async ({ data }) => {
  try {
    await init();
    const value =
      data.operation === "import"
        ? await import_source(data.path, data.bytes)
        : data.operation === "export"
          ? await export_source(data.path)
          : await inspect_archive(data.bytes, data.key);
    self.postMessage({ ok: true, value });
  } catch (error) {
    self.postMessage({ ok: false, error: String(error) });
  }
};
