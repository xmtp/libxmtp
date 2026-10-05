import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Use the declaration entry of the product selected by its source manifest.
export function sdkEntry(packageRoot, reference = false) {
  const manifest = JSON.parse(
    readFileSync(resolve(packageRoot, "package.json"), "utf8"),
  );
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
