import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs, ReleaseType } from "../types";
import { getSdkConfig } from "../lib/sdk-config";
import { resolveSdkVersion } from "../lib/sdk-version";
import { getTimestamp, validateTimestamp } from "../lib/version";
import { getShortSha } from "../lib/git";

export const command = "resolve-sdk-version";
export const describe =
  "Resolve an SDK's version for a release type given the pending libxmtp release";

export function builder(yargs: Argv<GlobalArgs>) {
  return (
    yargs
      .option("sdk", {
        type: "string",
        demandOption: true,
        describe: "SDK name",
      })
      // No "dev": this command is the track-aware path, only ever reached for
      // rc/final/nightly. A dev here would silently emit the legacy shape from
      // a pending version nothing has published yet.
      .option("releaseType", {
        type: "string",
        demandOption: true,
        choices: ["rc", "final", "nightly"] as const,
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
      .option("rcNumber", { type: "number" })
      .option("timestamp", {
        type: "string",
        describe:
          "Shared UTC YYYYMMDDHHMMSS stamp for this release run (defaults to now)",
      })
  );
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & {
      sdk: string;
      releaseType: ReleaseType;
      pendingVersion?: string;
      pendingKind?: "major" | "minor" | "patch";
      rcNumber?: number;
      timestamp?: string;
    }
  >,
) {
  const runTimestamp = validateTimestamp(argv.timestamp);
  if (!argv.pendingVersion || !argv.pendingKind) {
    throw new Error(
      "resolve-sdk-version requires --pending-version and --pending-kind " +
        "(the pending libxmtp release computed by the git-cliff oracle)",
    );
  }

  const config = getSdkConfig(argv.sdk);
  const base = config.manifest.readVersion(argv.repoRoot);
  const isNightly = argv.releaseType === "nightly";
  const shortSha = isNightly ? getShortSha(argv.repoRoot) : undefined;
  // Every SDK in one nightly run must land on the SAME pre.<ts> suffix, so use
  // the run stamp threaded in by release.yml; mint one only when standalone.
  const timestamp = isNightly ? (runTimestamp ?? getTimestamp()) : undefined;

  const version = resolveSdkVersion({
    track: config.versionTrack,
    base,
    pending: { version: argv.pendingVersion, kind: argv.pendingKind },
    releaseType: argv.releaseType,
    rcNumber: argv.rcNumber,
    timestamp,
    shortSha,
  });
  console.log(version);
}
