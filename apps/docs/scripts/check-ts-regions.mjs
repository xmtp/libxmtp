import ts from "typescript";
import { exampleConfig, docsRoot } from "./example-config.mjs";
import { validateExample } from "./example-regions.mjs";

const config = exampleConfig();
const program = ts.createProgram(config.fileNames, config.options);
for (const filename of config.fileNames)
  validateExample(program.getSourceFile(filename).text);
const errors = ts.getPreEmitDiagnostics(program);
if (errors.length) {
  console.error(
    ts.formatDiagnosticsWithColorAndContext(errors, {
      getCanonicalFileName: (path) => path,
      getCurrentDirectory: () => docsRoot,
      getNewLine: () => "\n",
    }),
  );
  process.exitCode = 1;
} else console.log(`OK: ${config.fileNames.length} SDK examples typecheck`);
