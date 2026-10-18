import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export function sdkEntry(packageRoot, subpath = ".") {
  const manifest = JSON.parse(
    readFileSync(resolve(packageRoot, "package.json"), "utf8"),
  );
  const types = manifest.exports?.[subpath]?.types;
  if (typeof types !== "string") {
    throw new Error(`SDK declaration export is missing: ${subpath}`);
  }
  return resolve(packageRoot, types);
}
