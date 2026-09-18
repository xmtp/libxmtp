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

test("spec preparation removes YAML frontmatter", () => {
  const result = prepareDocument(
    "---\nprefix: JOIN\nstatus: legacy\n---\n# Joining groups\n\nPublic text.",
    "004-join.md",
    { spec: true },
  );
  assert.equal(result.title, "Joining groups");
  assert.equal(result.body, "Public text.");
});

test("frontmatter-like text after the title is preserved", () => {
  const result = prepareDocument(
    "# Title\n\nIntro.\n\n---\n\nA horizontal rule stays.",
    "005.md",
    { spec: true },
  );
  assert.match(result.body, /horizontal rule stays/u);
});

test("spec preparation reports the frontmatter status", () => {
  const result = prepareDocument(
    "---\nprefix: JOIN\nstatus: draft\n---\n# Joining groups\n\nText.",
    "JOIN-joining-groups.md",
    { spec: true },
  );
  assert.equal(result.status, "draft");
  assert.equal(prepareDocument("# T\n\nBody.", "t.md").status, undefined);
});

test("relative links to sibling specs become routes", () => {
  const result = prepareDocument(
    "# Map\n\nSee [the format](SPEC-spec-format.md#scope) and [elsewhere](../other.md) and [web](https://x.example/a.md).",
    "README.md",
    {
      spec: true,
      resolveLink: (target) =>
        target === "SPEC-spec-format.md"
          ? "/specs/spec-spec-format/"
          : undefined,
    },
  );
  assert.equal(
    result.body,
    "See [the format](/specs/spec-spec-format/#scope) and [elsewhere](../other.md) and [web](https://x.example/a.md).",
  );
});

test("links inside code fences are not rewritten", () => {
  const result = prepareDocument(
    "# T\n\n```markdown\n[a](SPEC-spec-format.md)\n```",
    "t.md",
    { spec: true, resolveLink: () => "/specs/x/" },
  );
  assert.match(result.body, /\[a\]\(SPEC-spec-format\.md\)/u);
});
