import assert from "node:assert/strict";
import test from "node:test";

import { checkSearch } from "../scripts/check-search.mjs";
import { isSearchReference } from "../scripts/search-config.mjs";

test("specs and generated API references use reference search weights", () => {
  for (const id of [
    "specs",
    "specs/cons-consent",
    "specs/join-joining-groups",
    "reference/node-sdk/classes/Client",
    "reference/browser-sdk/classes/Client",
    "reference/agent-sdk/classes/Agent",
  ]) {
    assert.equal(isSearchReference(id), true, id);
  }
});

test("guides and curated reference pages keep guide search weights", () => {
  for (const id of [
    "sdk/consent",
    "sdk/conversations",
    "reference/limits",
    "reference/error-glossary",
    "protocol/overview",
    "specs-example",
  ]) {
    assert.equal(isSearchReference(id), false, id);
  }
});

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
