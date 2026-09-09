import ts from "typescript";
import { resolve } from "node:path";
import twoslash from "expressive-code-twoslash";
import { exampleRegions } from "./example-regions.mjs";

export const docsRoot = resolve(import.meta.dirname, "..");
export const configPath = resolve(docsRoot, "examples.tsconfig.json");

export function exampleConfig() {
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  if (config.error)
    throw new Error(
      ts.flattenDiagnosticMessageText(config.error.messageText, "\n"),
    );
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, docsRoot);
  if (parsed.errors.length)
    throw new Error(
      ts.formatDiagnosticsWithColorAndContext(parsed.errors, {
        getCanonicalFileName: (path) => path,
        getCurrentDirectory: () => docsRoot,
        getNewLine: () => "\n",
      }),
    );
  return parsed;
}

export function examplePlugins() {
  return [
    exampleRegions(),
    twoslash({
      cwd: docsRoot,
      tsConfigPath: configPath,
      instanceConfigs: { twoslash: { explicitTrigger: true } },
      twoslashOptions: {
        vfsRoot: docsRoot,
        compilerOptions: exampleConfig().options,
      },
    }),
  ];
}
