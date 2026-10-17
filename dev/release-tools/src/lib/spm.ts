import fs from "node:fs";
import path from "node:path";

// SwiftPM and CocoaPods read the same archive receipt.
export function updateSpmChecksum(
  packageSwiftPath: string,
  url: string,
  checksum: string,
): void {
  const artifact = new URL(url);
  if (!artifact.pathname.endsWith("/XmtpSdkFFI.zip")) {
    throw new Error("Expected the XmtpSdkFFI.zip release archive");
  }
  if (!/^[a-fA-F0-9]{64}$/.test(checksum)) {
    throw new Error("Expected the archive SHA-256 checksum");
  }
  fs.accessSync(packageSwiftPath, fs.constants.R_OK);
  const receipt = path.join(
    path.dirname(packageSwiftPath),
    "sdks/ios/ReleaseArtifacts.json",
  );
  fs.writeFileSync(
    receipt,
    JSON.stringify({ url, sha256: checksum.toLowerCase() }, null, 2) + "\n",
  );
}
