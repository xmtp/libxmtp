import fs from "node:fs";

// Matches the remote .binaryTarget declaration spanning multiple lines:
//   .binaryTarget(
//       name: "LibXMTPSwiftFFI",
//       url: "https://...",
//       checksum: "..."
//   )
// Uses the 's' (dotAll) flag so '.' matches newlines too.
const SPM_URL_REGEX =
  /(\.binaryTarget\(\s*name:\s*"LibXMTPSwiftFFI",\s*url:\s*)"([^"]+)"/s;

const SPM_CHECKSUM_REGEX = /(checksum:\s*)"([^"]+)"/s;

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
