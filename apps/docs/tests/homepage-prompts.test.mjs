import assert from "node:assert/strict";
import test from "node:test";
import { buildPrompt, prompts } from "../src/data/homepage-prompts.ts";

test("empty backend fields preserve the prototype prompts", () => {
  for (const id of Object.keys(prompts)) {
    assert.equal(buildPrompt(id), prompts[id]);
    assert.equal(buildPrompt(id, "  "), prompts[id]);
  }
});

test("a backend replaces only the discovery instruction", () => {
  for (const backend of [
    "https://backend.example.com",
    "http://127.0.0.1:5050",
  ]) {
    const result = buildPrompt("integration", backend);
    assert.ok(result.includes(new URL(backend).href));
    assert.ok(result.includes("secure local configuration"));
    assert.ok(!result.includes("Check whether my project"));
    assert.ok(result.endsWith("Report blockers honestly."));
  }
});

test("backend validation rejects unsafe or incomplete URL values", () => {
  for (const backend of [
    "example.com",
    "javascript:alert(1)",
    "ftp://example.com",
    "https://user:password@example.com",
    "https://example.com?token=secret",
    "https://example.com#secret",
    "https://example.com/a b",
    "https://example.com\n",
    "https://exam\tple.com",
    "https://example.com/\u0000",
  ]) {
    assert.throws(() => buildPrompt("integration", backend), Error);
  }
});
