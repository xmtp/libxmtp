import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { Application, ReflectionKind } from "typedoc";

test("references hide generated symbol brands and retain public members", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-typedoc-brands-"));
  try {
    const entry = join(root, "index.ts");
    const tsconfig = join(root, "tsconfig.json");
    await writeFile(
      tsconfig,
      JSON.stringify({ compilerOptions: { types: [] }, files: [entry] }),
    );
    await writeFile(
      entry,
      `
      declare const objectBrand: unique symbol;
      export declare class Conversations {
        readonly [objectBrand]: "Conversations";
        list(): string[];
      }
      export interface AppData { objectBrand: string; }
    `,
    );
    const app = await Application.bootstrapWithPlugins({
      name: "Brand fixture",
      entryPoints: [entry],
      tsconfig,
      plugin: [
        fileURLToPath(
          new URL("../scripts/typedoc-hide-brands.mjs", import.meta.url),
        ),
      ],
    });
    const project = await app.convert();
    assert.ok(project);
    assert.deepEqual(
      project
        .getReflectionsByKind(ReflectionKind.Property)
        .map(({ name }) => name),
      ["objectBrand"],
    );
    assert.deepEqual(
      project
        .getReflectionsByKind(ReflectionKind.Method)
        .map(({ name }) => name),
      ["list"],
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
