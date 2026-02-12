import type { ArgumentsCamelCase, Argv } from "yargs";
import {
  Sdk,
  BUMP_OPTIONS,
  type BumpOption,
  type BumpType,
  type GlobalArgs,
} from "../types";
import { bumpVersion } from "./bump-version";
import { setManifestVersion } from "./set-manifest-version";
import { scaffoldNotes } from "./scaffold-notes";
import { getSdkConfig } from "../lib/sdk-config";
import { tagExists } from "../lib/git";
import { execInherit } from "../lib/exec";

type SdkBump = {
  sdk: Sdk;
  bump: BumpType;
};

export const command = "create-release-branch";
export const describe =
  "Create a release branch with bumped versions and scaffolded notes";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "Release version number (used in branch name)",
    })
    .option("base", {
      type: "string",
      default: "HEAD",
      describe: "Base ref to branch from",
    })
    .option("ios", {
      type: "string",
      default: "none",
      choices: BUMP_OPTIONS,
      describe: "iOS SDK version bump type",
    })
    .option("android", {
      type: "string",
      default: "none",
      choices: BUMP_OPTIONS,
      describe: "Android SDK version bump type",
    })
    .option("node", {
      type: "boolean",
      default: false,
      describe: "Include Node bindings in release",
    })
    .option("wasm", {
      type: "boolean",
      default: false,
      describe: "Include WASM bindings in release",
    });
}

interface CreateReleaseBranchArgs extends GlobalArgs {
  version: string;
  base: string;
  ios: string;
  android: string;
  node: boolean;
  wasm: boolean;
}

export function handler(argv: ArgumentsCamelCase<CreateReleaseBranchArgs>) {
  const cwd = argv.repoRoot;
  const branchName = `release/${argv.version}`;

  // Collect SDK bumps to process
  const sdkBumps: Array<SdkBump> = [];

  if (argv.ios !== "none") {
    sdkBumps.push({ sdk: Sdk.Ios, bump: argv.ios as BumpType });
  }
  if (argv.android !== "none") {
    sdkBumps.push({ sdk: Sdk.Android, bump: argv.android as BumpType });
  }

  // Collect SDK includes (node/wasm just set the version directly)
  const sdkIncludes: Sdk[] = [];
  if (argv.node) {
    sdkIncludes.push(Sdk.NodeBindings);
  }
  if (argv.wasm) {
    sdkIncludes.push(Sdk.WasmBindings);
  }

  // Validate at least one SDK is being released
  if (sdkBumps.length === 0 && sdkIncludes.length === 0) {
    throw new Error(
      "At least one SDK must be bumped (use --ios or --android with a bump type, or --node/--wasm)",
    );
  }

  console.log(`Creating branch ${branchName} from ${argv.base}...`);
  execInherit(`git checkout -b ${branchName} ${argv.base}`, cwd);

  // Process each SDK
  const bumpedSdks: string[] = [];
  for (const { sdk, bump } of sdkBumps) {
    const config = getSdkConfig(sdk);
    const currentVersion = config.manifest.readVersion(cwd);
    const candidateTag = `${config.tagPrefix}${currentVersion}`;
    const sinceTag = tagExists(cwd, candidateTag) ? candidateTag : null;

    console.log(`Bumping ${sdk} version (${bump})...`);
    const newVersion = bumpVersion(sdk, bump, cwd);
    console.log(`New ${sdk} version: ${newVersion}`);

    console.log(`Scaffolding ${sdk} release notes...`);
    const notesPath = scaffoldNotes(sdk, cwd, currentVersion, sinceTag);
    console.log(`Release notes: ${notesPath}`);

    bumpedSdks.push(`${sdk} ${newVersion}`);
  }

  // Process node/wasm SDKs (set version directly, no semver bump)
  for (const sdk of sdkIncludes) {
    const config = getSdkConfig(sdk);
    const currentVersion = config.manifest.readVersion(cwd);
    const candidateTag = `${config.tagPrefix}${currentVersion}`;
    const sinceTag = tagExists(cwd, candidateTag) ? candidateTag : null;

    console.log(`Setting ${sdk} version to ${argv.version}...`);
    setManifestVersion(sdk, argv.version, cwd);

    console.log(`Scaffolding ${sdk} release notes...`);
    const notesPath = scaffoldNotes(sdk, cwd, currentVersion, sinceTag);
    console.log(`Release notes: ${notesPath}`);

    bumpedSdks.push(`${sdk} ${argv.version}`);
  }

  // Always set the libxmtp (Cargo.toml) version to the release version
  console.log(`Setting libxmtp version to ${argv.version}...`);
  setManifestVersion("libxmtp", argv.version, cwd);

  execInherit("git add -A", cwd);
  execInherit(
    `git commit -m "chore: create release ${argv.version} (${bumpedSdks.join(", ")})"`,
    cwd,
  );

  console.log(`Branch ${branchName} created and committed.`);
  console.log(`Push with: git push -u origin ${branchName}`);
}
