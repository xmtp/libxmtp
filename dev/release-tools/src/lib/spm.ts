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
  checksum: string,
): void {
  let content = fs.readFileSync(packageSwiftPath, "utf-8");

  if (!SPM_URL_REGEX.test(content)) {
    throw new Error(
      `Could not find remote binaryTarget url in ${packageSwiftPath}`,
    );
  }

  content = content.replace(SPM_URL_REGEX, `$1"${url}"`);
  if (!SPM_CHECKSUM_REGEX.test(content)) {
    throw new Error(`Could not find checksum field in ${packageSwiftPath}`);
  }
  content = content.replace(SPM_CHECKSUM_REGEX, `$1"${checksum}"`);

  fs.writeFileSync(packageSwiftPath, content);
}

const SPM_DYNAMIC_URL_REGEX =
  /(\.binaryTarget\(\s*name:\s*"LibXMTPSwiftFFIDynamic",\s*url:\s*)"([^"]+)"/s;

const SPM_DYNAMIC_CHECKSUM_REGEX =
  /(\.binaryTarget\(\s*name:\s*"LibXMTPSwiftFFIDynamic",[\s\S]*?checksum:\s*)"([^"]+)"/s;

export function updateSpmDynamicChecksum(
  packageSwiftPath: string,
  url: string,
  checksum: string,
): void {
  let content = fs.readFileSync(packageSwiftPath, "utf-8");

  if (!SPM_DYNAMIC_URL_REGEX.test(content)) {
    throw new Error(
      `Could not find remote dynamic binaryTarget url in ${packageSwiftPath}`,
    );
  }

  content = content.replace(SPM_DYNAMIC_URL_REGEX, `$1"${url}"`);
  if (!SPM_DYNAMIC_CHECKSUM_REGEX.test(content)) {
    throw new Error(
      `Could not find dynamic checksum field in ${packageSwiftPath}`,
    );
  }
  content = content.replace(SPM_DYNAMIC_CHECKSUM_REGEX, `$1"${checksum}"`);

  fs.writeFileSync(packageSwiftPath, content);
}
