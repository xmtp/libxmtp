// Keep diagram source for text exports without changing the visible page.
export function preserveMermaidSource() {
  return (tree) => {
    function visit(node) {
      if (!node.children) return;
      node.children = node.children.flatMap((child) => {
        const code = child.tagName === "pre" && child.children?.[0];
        if (code?.properties?.className?.includes("language-mermaid")) {
          const source = structuredClone(child);
          // The renderer must process only the visible copy.
          source.children[0].properties.className = ["language-llms-mermaid"];
          return [
            {
              type: "element",
              tagName: "div",
              properties: { className: ["llms-rendered-diagram"] },
              children: [child],
            },
            // The highlighter replaces the pre element, so hide its wrapper.
            {
              type: "element",
              tagName: "div",
              properties: {
                className: ["llms-diagram-source"],
                hidden: true,
                dataPagefindIgnore: "all",
              },
              children: [source],
            },
          ];
        }
        visit(child);
        return [child];
      });
    }
    visit(tree);
  };
}

export function restoreMermaidLanguage() {
  return (tree) => {
    function visit(node) {
      if (node.properties?.className?.includes("language-llms-mermaid")) {
        node.properties.className = ["language-mermaid"];
      }
      node.children?.forEach(visit);
    }
    visit(tree);
  };
}
