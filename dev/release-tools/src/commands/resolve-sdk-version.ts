import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs, ReleaseType } from "../types";
import { getSdkConfig } from "../lib/sdk-config";
import { resolveSdkVersion } from "../lib/sdk-version";
import { getShortSha } from "../lib/git";

export const command = "resolve-sdk-version";
export const describe =
  "Resolve an SDK's version for a release type given the pending libxmtp release";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", { type: "string", demandOption: true, describe: "SDK name" })
    .option("releaseType", {
      type: "string",
      demandOption: true,
      choices: ["dev", "rc", "final", "nightly"] as const,
    })
    // pending-* are only meaningful for the nightly/track-aware path. They are
    // optional at the CLI layer and validated in the handler so the signature
    // isn't misleading for callers; the handler errors if they're missing.
    .option("pendingVersion", {
      type: "string",
      describe: "Pending libxmtp version (e.g. 1.11.0) — required",
    })
    .option("pendingKind", {
      type: "string",
      choices: ["major", "minor", "patch"] as const,
      describe: "Pending libxmtp bump kind — required",
    })
    .option("rcNumber", { type: "number" });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & {
      sdk: string;
      releaseType: ReleaseType;
      pendingVersion?: string;
      pendingKind?: "major" | "minor" | "patch";
      rcNumber?: number;
    }
  >,
) {
  if (!argv.pendingVersion || !argv.pendingKind) {
    throw new Error(
      "resolve-sdk-version requires --pending-version and --pending-kind " +
        "(the pending libxmtp release computed by the git-cliff oracle)",
    );
  }

  const config = getSdkConfig(argv.sdk);
  const base = config.manifest.readVersion(argv.repoRoot);
  const needsSha = argv.releaseType === "dev" || argv.releaseType === "nightly";
  const shortSha = needsSha ? getShortSha(argv.repoRoot) : undefined;
  const nightlyDate =
    argv.releaseType === "nightly"
      ? new Date().toISOString().slice(0, 10).replace(/-/g, "")
      : undefined;

  const version = resolveSdkVersion({
    track: config.versionTrack,
    base,
    pending: { version: argv.pendingVersion, kind: argv.pendingKind },
    releaseType: argv.releaseType,
    rcNumber: argv.rcNumber,
    nightlyDate,
    shortSha,
  });
  console.log(version);
}
