import assert from "node:assert/strict";
import test from "node:test";
import { prepareDocument } from "../src/loaders/prepare.mjs";

test("spec preparation removes a review record through the next H2", () => {
  const result = prepareDocument(
    "# API behavior\n\nIntro.\n\n## Review record\n\nInternal approval.\n\n### Notes\n\nMore approval.\n\n## Requirements\n\nPublic text.",
    "001-api.md",
    { spec: true },
  );
  assert.equal(result.title, "API behavior");
  assert.equal(result.order, 1);
  assert.equal(result.body, "Intro.\n\n## Requirements\n\nPublic text.");
});

test("spec preparation removes a review log through end of file", () => {
  const result = prepareDocument(
    "# Title\n\nPublic.\n\n## Review log\n\nPrivate.",
    "002.md",
    { spec: true },
  );
  assert.equal(result.body, "Public.");
});

test("review headings inside code fences stay intact", () => {
  const result = prepareDocument(
    "# Title\n\n~~~~markdown\n## Review record\nkeep this example\n~~~~\n\n## Result\nDone.",
    "example.md",
    { spec: true },
  );
  assert.match(result.body, /## Review record\nkeep this example/);
  assert.match(result.body, /## Result/);
});

test("the first H1 becomes the title and is removed from the body", () => {
  const result = prepareDocument(
    "Lead text.\n\n# Lifted title\n\nBody.",
    "guide.md",
  );
  assert.equal(result.title, "Lifted title");
  assert.equal(result.body, "Lead text.\n\n\nBody.");
});

test("a missing H1 is a build error", () => {
  assert.throws(
    () => prepareDocument("## Section", "broken.md"),
    /Missing document title: broken\.md/,
  );
});
