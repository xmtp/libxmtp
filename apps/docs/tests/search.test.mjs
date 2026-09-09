import assert from "node:assert/strict";
import test from "node:test";
import { checkSearch } from "../scripts/check-search.mjs";

test("search check reports a wrong first result", async () => {
  let initialized = false;
  const pagefind = {
    init: async () => {
      initialized = true;
    },
    search: async () => ({
      results: [{ data: async () => ({ url: "/wrong/" }) }],
    }),
  };
  assert.deepEqual(await checkSearch(pagefind, [["quickstart", "/right/"]]), [
    "quickstart: got /wrong/; expected /right/",
  ]);
  assert.equal(initialized, true);
});
