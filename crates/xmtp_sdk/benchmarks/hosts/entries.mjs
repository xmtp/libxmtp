import { readFile, realpath } from "node:fs/promises";
import { dirname, join, relative, isAbsolute } from "node:path";

function selectExport(value, conditions) {
  if (typeof value === "string" || value === null) return value;
  if (!value || Array.isArray(value))
    throw new Error("Unsupported installed public export declaration");
  for (const [condition, entry] of Object.entries(value)) {
    if (condition === "default" || conditions.has(condition)) {
      const selected = selectExport(entry, conditions);
      if (selected !== undefined) return selected;
    }
  }
}

// Admit the installed package's public ESM exports before loading host tools.
export async function admitEntries(config, request, target) {
  const closure = await realpath(request.package_root);
  const inside = (path) => {
    const suffix = relative(closure, path);
    if (suffix.startsWith("..") || isAbsolute(suffix))
      throw new Error("SDK entry is outside the recorded package closure");
  };
  const sdk = await realpath(config.sdk_entry);
  inside(sdk);
  if (
    !["old", "new"].includes(request.side) ||
    !["node", "browser"].includes(target)
  )
    throw new Error("Unknown benchmark package side or target");
  let directory = dirname(sdk);
  let manifest;
  while (true) {
    inside(directory);
    try {
      manifest = JSON.parse(
        await readFile(join(directory, "package.json"), "utf8"),
      );
      break;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    if (directory === closure)
      throw new Error("Missing installed SDK manifest");
    directory = dirname(directory);
  }
  const expectedName =
    request.side === "old"
      ? `@xmtp/${target}-sdk`
      : target === "node"
        ? "xmtp-sdk"
        : "xmtp-sdk-browser";
  if (manifest.name !== expectedName || manifest.type !== "module")
    throw new Error("Unexpected installed SDK package identity");
  const conditions = new Set(
    target === "node"
      ? ["import", "node", "node-addons"]
      : ["import", "browser", "production", "module"],
  );
  if (target === "node") {
    const args = [
      ...process.execArgv,
      ...(process.env.NODE_OPTIONS ?? "").split(/\s+/),
    ];
    for (let index = 0; index < args.length; index++) {
      const value = args[index].startsWith("--conditions=")
        ? args[index].slice(13)
        : ["--conditions", "-C"].includes(args[index])
          ? args[++index]
          : undefined;
      if (value)
        for (const condition of value.split(",")) conditions.add(condition);
    }
  }
  const publicPath = async (subpath) => {
    const exports = manifest.exports;
    const declaration =
      typeof exports === "string"
        ? subpath === "."
          ? exports
          : undefined
        : exports?.[subpath];
    const entry = selectExport(declaration, conditions);
    if (typeof entry !== "string" || !entry.startsWith("./"))
      throw new Error(`Missing installed public ESM export ${subpath}`);
    const path = await realpath(join(directory, entry));
    inside(path);
    return path;
  };
  if (sdk !== (await publicPath(".")))
    throw new Error("SDK entry does not identify the installed public root");
  if (target === "node" || request.side === "old") {
    if (config.pure_entry && (await realpath(config.pure_entry)) !== sdk)
      throw new Error("Codecs must use the installed public root");
  } else {
    if (
      !config.pure_entry ||
      (await realpath(config.pure_entry)) !== (await publicPath("./pure"))
    )
      throw new Error(
        "Browser codecs must use the installed public ./pure export",
      );
  }
}
