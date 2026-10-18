import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative } from "node:path";

export const readJson = (path) => JSON.parse(readFileSync(path));

export function writeJson(path, value, formatted = false) {
  writeFileSync(
    path,
    JSON.stringify(value, null, formatted ? 2 : undefined) +
      (formatted ? "\n" : ""),
  );
}

export const hash = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

export function files(directory, includeNodeModules = false) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((item) => {
    if (!includeNodeModules && item.name === "node_modules") return [];
    const path = join(directory, item.name);
    if (item.isSymbolicLink())
      throw new Error(`package input contains a symbolic link: ${path}`);
    return item.isDirectory() ? files(path, includeNodeModules) : [path];
  });
}

// Copies of Nix store paths keep their read-only modes.
export function makeWritable(directory) {
  chmodSync(directory, 0o755);
  for (const item of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, item.name);
    if (item.isSymbolicLink())
      throw new Error(`copied SDK input has a symbolic link: ${path}`);
    if (item.isDirectory()) makeWritable(path);
    else chmodSync(path, 0o644);
  }
}

// Open directories without following links so failed-copy cleanup stays safe.
export function removeTree(directory) {
  if (!existsSync(directory)) return;
  const open = (path) => {
    chmodSync(path, 0o755);
    for (const item of readdirSync(path, { withFileTypes: true }))
      if (item.isDirectory()) open(join(path, item.name));
  };
  if (!lstatSync(directory).isSymbolicLink()) open(directory);
  rmSync(directory, { recursive: true, force: true });
}

export function rewriteImports(path, bindings) {
  let source = readFileSync(path, "utf8");
  for (const [name, entry] of Object.entries(bindings)) {
    let specifier = relative(dirname(path), entry).replaceAll("\\", "/");
    if (!specifier.startsWith(".")) specifier = `./${specifier}`;
    source = source
      .replaceAll(JSON.stringify(name), JSON.stringify(specifier))
      .replaceAll(`'${name}'`, JSON.stringify(specifier));
  }
  writeFileSync(path, source);
}

export function writeEntry(directory, name, entry) {
  for (const extension of ["js", "d.ts"])
    writeFileSync(
      join(directory, `${name}.${extension}`),
      `export * from '${entry}';\n`,
    );
}

// Build beside the product so replacement and rollback use atomic renames.
export function stagePackage(product, build) {
  mkdirSync(dirname(product), { recursive: true });
  const staging = mkdtempSync(join(dirname(product), ".sdk-stage-"));
  const destination = join(staging, "product");
  const previous = join(staging, "previous");
  try {
    const result = build(destination, staging);
    // Keep the prior product until compilation and all package checks succeed.
    if (existsSync(product)) renameSync(product, previous);
    try {
      renameSync(destination, product);
    } catch (error) {
      if (existsSync(previous)) {
        try {
          renameSync(previous, product);
        } catch (rollbackError) {
          throw new AggregateError(
            [error, rollbackError],
            `Previous SDK product remains at ${previous}`,
          );
        }
      }
      throw error;
    }
    removeTree(previous);
    return result;
  } finally {
    // A failed rollback leaves the prior product here for manual recovery.
    if (!existsSync(previous)) removeTree(staging);
  }
}
