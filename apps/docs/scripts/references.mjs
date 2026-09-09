import { access, cp, mkdir, rm, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export async function installReferences({
  repositoryRoot = resolve(import.meta.dirname, "../../.."),
  generatedRoot = resolve(repositoryRoot, "apps/docs/generated/reference"),
  siteRoot = resolve(repositoryRoot, "apps/docs/_site"),
} = {}) {
  const references = [
    {
      name: "Rust",
      source: resolve(generatedRoot, "rust"),
      destination: resolve(siteRoot, "rust"),
    },
    {
      name: "Kotlin",
      source: resolve(generatedRoot, "kotlin"),
      destination: resolve(siteRoot, "reference/kotlin"),
    },
    {
      name: "Swift",
      source: resolve(generatedRoot, "swift"),
      destination: resolve(siteRoot, "reference/swift"),
    },
  ];

  await mkdir(siteRoot, { recursive: true });

  for (const reference of references) {
    try {
      await access(reference.source, constants.R_OK);
    } catch {
      throw new Error(
        `${reference.name} API reference is missing: ${reference.source}`,
      );
    }

    await rm(reference.destination, { recursive: true, force: true });
    await mkdir(reference.destination, { recursive: true });
    await cp(reference.source, reference.destination, { recursive: true });
  }

  const rustCrateIndex = resolve(siteRoot, "rust/xmtp_mls/index.html");
  await access(rustCrateIndex, constants.R_OK).catch(() => {
    throw new Error(`Rust crate index is missing: ${rustCrateIndex}`);
  });
  await writeFile(
    resolve(siteRoot, "rust/index.html"),
    '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=./xmtp_mls/"><link rel="canonical" href="./xmtp_mls/"><title>XMTP Rust API reference</title><p><a href="./xmtp_mls/">Open the XMTP Rust API reference</a>.</p>\n',
  );

  const swiftModuleIndex = resolve(
    siteRoot,
    "reference/swift/documentation/xmtpios/index.html",
  );
  await access(swiftModuleIndex, constants.R_OK).catch(() => {
    throw new Error(`Swift module index is missing: ${swiftModuleIndex}`);
  });
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await installReferences();
}
