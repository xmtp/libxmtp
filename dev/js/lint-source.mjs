import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const packages = JSON.parse(
  execFileSync("pnpm", ["-r", "list", "--depth", "-1", "--json"], {
    cwd: root,
    encoding: "utf8",
  }),
);
const temporary = mkdtempSync(join(tmpdir(), "xmtp-source-lint-"));
const sourceOptions = {
  typeAware: false,
  // Type-rule suppressions can only be judged in the full check.
  reportUnusedDisableDirectives: "off",
};
try {
  const helperConfig = join(temporary, "helper.json");
  writeFileSync(
    helperConfig,
    JSON.stringify({
      extends: [join(root, ".oxlintrc.json")],
      options: sourceOptions,
    }),
  );
  execFileSync(
    "pnpm",
    ["exec", "oxlint", "-c", helperConfig, "dev/js/lint-source.mjs"],
    { cwd: root, stdio: "inherit" },
  );
  for (const [index, pkg] of packages.entries()) {
    if (pkg.path === root) continue;
    if (pkg.name === "@xmtp/docs") {
      // Docs has source Oxlint and Markdown checks in its existing command.
      execFileSync("pnpm", ["run", "lint:prepared"], {
        cwd: pkg.path,
        stdio: "inherit",
      });
      continue;
    }
    const localConfig = join(pkg.path, ".oxlintrc.json");
    const config = join(temporary, `${index}.json`);
    // Keep the shared rules and package overrides. The required full check
    // runs type rules against current products with the original config.
    writeFileSync(
      config,
      JSON.stringify({
        extends: [
          existsSync(localConfig) ? localConfig : join(root, ".oxlintrc.json"),
        ],
        options: sourceOptions,
      }),
    );
    execFileSync(
      "pnpm",
      ["exec", "oxlint", "--disable-nested-config", "-c", config, "."],
      { cwd: pkg.path, stdio: "inherit" },
    );
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
