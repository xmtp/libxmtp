import assert from "node:assert/strict";
import test from "node:test";
import { createMarkdownProcessor } from "@astrojs/markdown-remark";
import rehypeMermaid from "rehype-mermaid";
import {
  preserveMermaidSource,
  restoreMermaidLanguage,
} from "../scripts/llms-diagrams.mjs";

test("diagram exports retain source without rendering it twice", () => {
  const diagram = {
    type: "element",
    tagName: "pre",
    properties: {},
    children: [
      {
        type: "element",
        tagName: "code",
        properties: { className: ["language-mermaid"] },
        children: [{ type: "text", value: "flowchart LR\n  A --> B\n" }],
      },
    ],
  };
  const ordinary = structuredClone(diagram);
  ordinary.children[0].properties.className = ["language-ts"];
  const tree = { type: "root", children: [diagram, ordinary] };
  preserveMermaidSource()(tree);
  assert.equal(tree.children.length, 3);
  assert.equal(tree.children[0].children[0], diagram);
  assert.deepEqual(tree.children[0].properties.className, [
    "llms-rendered-diagram",
  ]);
  const wrapper = tree.children[1];
  assert.equal(wrapper.properties.hidden, true);
  assert.equal(wrapper.properties.dataPagefindIgnore, "all");
  const source = wrapper.children[0];
  assert.deepEqual(source.children[0].properties.className, [
    "language-llms-mermaid",
  ]);
  restoreMermaidLanguage()(tree);
  assert.deepEqual(source.children[0].properties.className, [
    "language-mermaid",
  ]);
  assert.deepEqual(source.children[0].children, diagram.children[0].children);
  assert.equal(tree.children[2], ordinary);
});

test("Markdown rendering keeps a hidden source copy beside the visible diagram", async () => {
  const processor = await createMarkdownProcessor({
    syntaxHighlight: false,
    rehypePlugins: [
      preserveMermaidSource,
      [rehypeMermaid, { strategy: "pre-mermaid" }],
      restoreMermaidLanguage,
    ],
  });
  const { code } = await processor.render(
    "```mermaid\nflowchart LR\n  A --> B\n```\n",
  );
  assert.match(code, /class="llms-rendered-diagram"><pre class="mermaid"/);
  assert.match(
    code,
    /<div class="llms-diagram-source" hidden data-pagefind-ignore="all"><pre><code class="language-mermaid">flowchart LR/,
  );
  assert.doesNotMatch(code, /language-llms-mermaid/);
});
