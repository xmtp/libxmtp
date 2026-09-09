import test from "node:test";
import assert from "node:assert/strict";
import {
  checkSources,
  findUndocumented,
} from "../scripts/agent-doc-coverage.mjs";

test("Agent SDK public declarations have TSDoc", async () => {
  assert.deepEqual(await findUndocumented(), []);
});

test("coverage detects undocumented exported declarations and members", () => {
  const files = new Map([
    [
      "/fixture/index.ts",
      "export class Public { public value = 1; private hidden = 2; method() {} }",
    ],
  ]);
  assert.equal(checkSources(files).length, 3);
});

test("coverage accepts documented declarations and excludes private members", () => {
  const files = new Map([
    [
      "/fixture/index.ts",
      `/** Public API. */
export class Public {
  /** Value. */
  public value = 1;
  private hidden = 2;
  /** Does work. */
  method() {}
}`,
    ],
  ]);
  assert.deepEqual(checkSources(files), []);
});

test("coverage resolves cross-file re-exports with JavaScript extensions", () => {
  const sources = new Map([
    ["/fixture/index.ts", 'export { Foo } from "./foo.js";'],
    ["/fixture/foo.ts", "export class Foo { missing() {} }"],
  ]);
  assert.equal(checkSources(sources).length, 2);
});

test("coverage resolves local named aliases", () => {
  assert.equal(
    checkSources(
      new Map([
        [
          "/fixture/index.ts",
          "const missing = 1; export { missing as value };",
        ],
      ]),
    ).length,
    1,
  );
});

test("coverage follows public base types and parameter types", () => {
  const source = `
/** Event names. */
type Events = { missingEvent: [number] };
/** Base type. */
class Base<T> {}
/** Options. */
interface Options { missingOption: boolean }
/** API. */
export class API extends Base<Events> {
  /** Do work. */
  method(options: Options): void {}
}`;
  const missing = checkSources(new Map([["/fixture/index.ts", source]]));
  assert.equal(missing.length, 2);
  assert.ok(missing.some((line) => line.includes("missingEvent")));
  assert.ok(missing.some((line) => line.includes("missingOption")));
});

test("coverage rejects empty docs and public constructors", () => {
  assert.equal(
    checkSources(
      new Map([
        ["/fixture/index.ts", "/** */ export class API { constructor() {} }"],
      ]),
    ).length,
    2,
  );
});

test("coverage checks object members and accepts documented shorthand", () => {
  const source = `const helper = () => true;
/** Predicates. */
export const filter = {
  /** Check value. */
  helper,
  missing: () => false,
};`;
  const missing = checkSources(new Map([["/fixture/index.ts", source]]));
  assert.equal(missing.length, 1);
  assert.match(missing[0], /missing$/u);
});

test("coverage does not include private state or unexported declarations", () => {
  const source = `type Internal = { missing: string };
class Hidden { missing() {} }
/** Public API. */
export class API {
  #state: Internal;
  private privateValue: Internal;
  protected protectedValue: Internal;
  /** Do work. */
  static async run(): Promise<void> {}
}`;
  assert.deepEqual(checkSources(new Map([["/fixture/index.ts", source]])), []);
});

test("coverage follows arrow signatures and inferred return types", () => {
  const source = `
/** Options. */
interface Options { missing: string }
/** Result. */
class Result { undocumented = 1 }
/** Run. */
export const run = (options: Options) => new Result();`;
  const missing = checkSources(new Map([["/fixture/index.ts", source]]));
  assert.equal(missing.length, 2);
  assert.ok(missing.some((line) => line.endsWith("missing")));
  assert.ok(missing.some((line) => line.endsWith("undocumented")));
});

test("coverage checks nested objects and exported object aliases", () => {
  const source = `
const inner = { missing: 1 };
/** API. */
export const api = {
  /** Nested API. */
  nested: inner,
};
/** Alias. */
export const alias = inner;`;
  const missing = checkSources(new Map([["/fixture/index.ts", source]]));
  assert.equal(missing.length, 1);
  assert.match(missing[0], /missing$/u);
});

test("coverage follows arrow predicates and generic constraints and defaults", () => {
  for (const declaration of [
    "export const api = (value: unknown): value is Hidden => true;",
    "export const api = (value: unknown) => value instanceof Hidden;",
    "export const api = <T extends Hidden>() => null;",
    "export const api = <T = Hidden>() => null;",
    "export const api = {\n/** Factory. */\ncreate: <T extends Hidden>() => null };",
    "export const api = function <T extends Hidden>() { return null; };",
  ]) {
    const source = `class Hidden { missing = 1 }\n/** API. */\n${declaration}`;
    const missing = checkSources(new Map([["/fixture/index.ts", source]]));
    assert.equal(missing.length, 2, declaration);
    assert.ok(
      missing.some((line) => line.endsWith("Hidden")),
      declaration,
    );
    assert.ok(
      missing.some((line) => line.endsWith("missing")),
      declaration,
    );
  }
});
