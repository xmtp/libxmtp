import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const packages = JSON.parse(
  execFileSync("pnpm", ["-r", "list", "--depth", "-1", "--json"], {
    cwd: root,
    encoding: "utf8",
  }),
);
const temporaryConfigs = [];
const token = randomUUID();
const sourceOptions = {
  typeAware: false,
  // Type-rule suppressions can only be judged in the full check.
  reportUnusedDisableDirectives: "off",
};
function sourceConfig(inheritedConfig, index) {
  // Oxlint does not inherit ignore patterns through an extends-only wrapper.
  // Resolve the original config and keep its directory for relative scopes.
  const resolved = JSON.parse(
    execFileSync(
      "pnpm",
      ["exec", "oxlint", "--print-config", "-c", inheritedConfig],
      {
        cwd: root,
        encoding: "utf8",
      },
    ),
  );
  const config = join(
    dirname(inheritedConfig),
    `.oxlint-source-${token}-${index}.json`,
  );
  temporaryConfigs.push(config);
  writeFileSync(
    config,
    JSON.stringify({
      extends: [inheritedConfig],
      ignorePatterns: resolved.ignorePatterns,
      options: sourceOptions,
    }),
  );
  return config;
}
try {
  const helperConfig = sourceConfig(join(root, ".oxlintrc.json"), "helper");
  execFileSync(
    "pnpm",
    ["exec", "oxlint", "-c", helperConfig, "dev/js/lint-source.mjs"],
    { cwd: root, stdio: "inherit" },
  );
  for (const [index, pkg] of packages.entries()) {
    if (pkg.path === root) continue;
    const localConfig = join(pkg.path, ".oxlintrc.json");
    // Keep the shared rules and package overrides. The required full check
    // runs type rules against current products with the original config.
    const config = sourceConfig(
      existsSync(localConfig) ? localConfig : join(root, ".oxlintrc.json"),
      index,
    );
    execFileSync(
      "pnpm",
      ["exec", "oxlint", "--disable-nested-config", "-c", config, "."],
      { cwd: pkg.path, stdio: "inherit" },
    );
  }
} finally {
  for (const config of temporaryConfigs) rmSync(config, { force: true });
}
