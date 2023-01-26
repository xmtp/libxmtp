import { expect, it } from "vitest";
import { Wasmpkg } from "..";

it("can instantiate parser", async () => {
  const a = await Wasmpkg.initialize();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await Wasmpkg.initialize();
  expect(b).toBeDefined();
});

it("should handle the simple case", async () => {
  // Make sure we can call it again
  const b = await Wasmpkg.initialize();
  expect(b).toBeDefined();
  expect(await b.add(5, 3)).toEqual(8);
});

it("should handle the simple case", async () => {
  // Make sure we can call it again
  const b = await Wasmpkg.initialize();
  expect(b).toBeDefined();
  expect(await b.add(-4, 0)).toEqual(-4);
});
