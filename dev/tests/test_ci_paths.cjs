// Exercise the production filters with the pinned Dorny matcher.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const Module = require("node:module");

const root = path.resolve(__dirname, "../..");
const bundlePath = process.env.CI_PATH_FILTER_BUNDLE;
assert.ok(bundlePath, "Run this fixture in the Nix development shell");
const pin = "ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d";
const source = fs.readFileSync(bundlePath, "utf8");
const entry = "var __webpack_exports__ = __nccwpck_require__(3109);";
assert.equal(source.split(entry).length, 2);
// Expose the bundled matcher. Its code and dependencies stay unchanged.
const library = new Module(bundlePath, module);
library.filename = bundlePath;
library.paths = module.paths;
library._compile(
  source.replace(
    entry,
    "var __webpack_exports__ = { ...__nccwpck_require__(3707), loadYaml: __nccwpck_require__(1917).load };",
  ),
  bundlePath,
);
const { Filter, loadYaml } = library.exports;
const workflow = loadYaml(
  fs.readFileSync(path.join(root, ".github/workflows/ci.yml"), "utf8"),
);
const action = workflow.jobs["detect-changes"].steps.find(
  (step) => step.id === "paths",
);
assert.equal(action.uses, `dorny/paths-filter@${pin}`);
const filter = new Filter(
  fs.readFileSync(path.join(root, action.with.filters), "utf8"),
  {
    predicateQuantifier: action.with["predicate-quantifier"],
  },
);
const js = [
  "lint_js",
  "lint_config",
  "test_node",
  "test_agent",
  "test_browser",
  "test_bridge_runtime",
  "test_browser_platform",
  "check_types",
  "check_sdk",
  "docs_quality",
].sort();
const android = [
  "lint_android",
  "test_sdk_staging",
  "check_bindings_android",
  "test_android",
  "test_android_consumers",
  "test_android_platform",
  "docs_quality",
].sort();
const fixtures = [];
for (const filename of [
  "docs/specs/OPS-backend-operations.md",
  "docs/schemas/example.json",
  "docs/examples/Cargo.toml",
  "docs/examples/build.gradle.kts",
  "docs/examples/package.json",
  "docs/examples/build.rs",
  "docs/diagram.png",
]) {
  fixtures.push({
    paths: [filename],
    checks: ["docs_quality", "docs_site"],
    reasons: [],
  });
}
fixtures.push({ paths: ["sdks/js.just"], checks: js, reasons: [] });
for (const filename of [
  "sdks/android/build.gradle",
  "sdks/android/settings.gradle.kts",
  "apps/kotlin-messenger/build.gradle.kts",
  "dev/examples/build.gradle",
]) {
  fixtures.push({ paths: [filename], checks: android, reasons: [] });
}
fixtures.push({
  paths: ["docs/examples/Cargo.toml", "sdks/js.just"],
  checks: [...js, "docs_site"].sort(),
  reasons: [],
});
fixtures.push({
  paths: ["docs/examples/build.gradle.kts", "sdks/android/build.gradle"],
  checks: [...android, "docs_site"].sort(),
  reasons: [],
});
for (const filename of [
  "Cargo.toml",
  "apps/backend/src/error.rs",
  "dev/docker/up",
]) {
  fixtures.push({ paths: [filename], checks: null, reasons: ["shared_input"] });
}
fixtures.push({
  paths: ["new-language/source.xyz"],
  checks: null,
  reasons: ["unknown_paths"],
});
const cases = fixtures.map((fixture) => {
  const matched = filter.match(
    fixture.paths.map((filename) => ({ filename, status: "modified" })),
  );
  return {
    ...fixture,
    outputs: {
      changes: JSON.stringify(
        Object.keys(matched).filter((key) => matched[key].length),
      ),
      all_files_files: JSON.stringify(
        matched.all_files.map((file) => file.filename),
      ),
      known_files_files: JSON.stringify(
        matched.known_files.map((file) => file.filename),
      ),
      shared_files: JSON.stringify(matched.shared.map((file) => file.filename)),
    },
  };
});
const bridge = `import json,runpy,sys
ns=runpy.run_path(sys.argv[1])
event={'repository':{'full_name':'xmtp/libxmtp'},'pull_request':{'draft':False,'head':{'repo':{'full_name':'xmtp/libxmtp'}}}}
for case in json.load(sys.stdin):
 result=ns['select_checks'](case['outputs'],'pull_request',event,'success',len(case['paths']))
 selected=sorted(k for k,v in result['plan']['checks'].items() if v and k in ns['CHECK_CATALOG'])
 expected=case['checks'] if case['checks'] is not None else sorted({c.flag or k for k,c in ns['CHECK_CATALOG'].items()})
 assert selected==expected,(case['paths'],selected,expected)
 assert result['reasons']==case['reasons'],(case['paths'],result['reasons'])
print(str(${cases.length})+' pinned-matcher routing cases passed')
`;
const result = spawnSync(
  "python3.11",
  ["-B", "-c", bridge, path.join(root, "dev/ci-select")],
  {
    input: JSON.stringify(cases),
    encoding: "utf8",
  },
);
assert.equal(result.status, 0, result.stderr);
process.stdout.write(result.stdout);
