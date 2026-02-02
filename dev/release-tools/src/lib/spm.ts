import fs from "node:fs";

// Matches the url line in a .binaryTarget declaration:
//   url: "https://...",
const SPM_URL_REGEX =
  /(\.binaryTarget\(\s*name:\s*"LibXMTPSwiftFFI",\s*url:\s*)"([^"]+)"/;

// Matches the checksum line:
//   checksum: "..."
const SPM_CHECKSUM_REGEX = /(checksum:\s*)"([^"]+)"/;

export function updateSpmChecksum(
  packageSwiftPath: string,
  url: string,
  checksum: string
): void {
  let content = fs.readFileSync(packageSwiftPath, "utf-8");

  if (!SPM_URL_REGEX.test(content)) {
    throw new Error(
      `Could not find remote binaryTarget url in ${packageSwiftPath}`
    );
  }

  content = content.replace(SPM_URL_REGEX, `$1"${url}"`);
  content = content.replace(SPM_CHECKSUM_REGEX, `$1"${checksum}"`);

  fs.writeFileSync(packageSwiftPath, content);
}
