import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Use the declaration entry of the product selected by its source manifest.
export function sdkEntry(packageRoot, reference = false, subpath = ".") {
  const manifest = JSON.parse(
    readFileSync(resolve(packageRoot, "package.json"), "utf8"),
  );
  if (subpath !== ".") {
    const types = manifest.exports?.[subpath]?.types;
    if (typeof types !== "string") {
      throw new Error(`SDK declaration export is missing: ${subpath}`);
    }
    return resolve(packageRoot, types);
  }
  const generated = manifest.scripts?.build?.includes("dev/js/sdk-package");
  return resolve(
    packageRoot,
    generated
      ? "dist/entry.d.ts"
      : reference
        ? "src/index.ts"
        : "dist/index.d.ts",
  );
}
