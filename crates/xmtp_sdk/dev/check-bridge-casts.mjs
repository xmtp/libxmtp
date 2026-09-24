import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const compiler = readdirSync("node_modules/.pnpm")
  .filter((name) => name.startsWith("typescript@"))
  .map((name) =>
    resolve(
      "node_modules/.pnpm",
      name,
      "node_modules/typescript/lib/typescript.js",
    ),
  )
  .find(existsSync);
if (!compiler)
  throw new Error("TypeScript parser is missing: run just install");
const { default: ts } = await import(pathToFileURL(compiler).href);

function files(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const name = join(path, entry.name);
    return entry.isDirectory()
      ? files(name)
      : name.endsWith(".ts")
        ? [name]
        : [];
  });
}

const generated = "target/sdk-generated/typescript-wasm";
const paths = [
  ...files("apps/xmtp_sdk_bindgen/runtime/ts/bridge"),
  ...readdirSync(generated)
    .filter((name) => /\.gen(?:\.test)?\.ts$/.test(name))
    .map((name) => join(generated, name)),
];
let failed = false;
for (const path of paths) {
  const source = ts.createSourceFile(
    path,
    readFileSync(path, "utf8"),
    ts.ScriptTarget.Latest,
    true,
  );
  function visit(node) {
    if (ts.isAsExpression(node) || ts.isTypeAssertionExpression(node)) {
      const { line } = source.getLineAndCharacterOfPosition(
        node.getStart(source),
      );
      console.error(`${path}:${line + 1}: bridge type assertion is forbidden`);
      failed = true;
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
if (failed) process.exitCode = 1;
