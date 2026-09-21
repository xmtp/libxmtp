const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---\r?\n/u;

export function prepareDocument(
  raw,
  filename,
  { spec = false, resolveLink } = {},
) {
  // Specs carry YAML frontmatter that names their prefix and status. The site
  // reads the status for a sidebar badge; the block itself is not published.
  const frontmatter = raw.match(FRONTMATTER);
  const status = frontmatter?.[1].match(/^status:\s*(\S+)/mu)?.[1];
  const withoutFrontmatter = frontmatter
    ? raw.slice(frontmatter[0].length)
    : raw;
  const lines = withoutFrontmatter.split(/\r?\n/u);
  let inReview = false;
  let fence;
  const kept = [];
  for (let line of lines) {
    const marker = line.match(/^\s{0,3}(`{3,}|~{3,})/u)?.[1];
    if (marker) {
      if (!fence) fence = marker;
      else if (marker[0] === fence[0] && marker.length >= fence.length)
        fence = undefined;
    }
    if (spec && !fence && /^##\s+/u.test(line)) {
      inReview = /^##\s+Review (?:record|log)\s*$/iu.test(line);
    }
    // A spec links to its neighbours by file name. The site publishes them
    // under routes, so the loader supplies the mapping.
    if (resolveLink && !fence && !marker)
      line = line.replace(
        /\]\(([^)\s:#/][^)\s:#]*\.md)(#[^)\s]*)?\)/gu,
        (match, target, anchor = "") => {
          const route = resolveLink(target);
          return route ? `](${route}${anchor})` : match;
        },
      );
    if (!inReview && !(spec && !fence && /^Status:/u.test(line)))
      kept.push(line);
  }
  const titleIndex = kept.findIndex((line) => /^#\s+/u.test(line));
  if (titleIndex < 0) throw new Error(`Missing document title: ${filename}`);
  const title = kept.splice(titleIndex, 1)[0].replace(/^#\s+/u, "");
  return {
    title,
    status,
    body: kept.join("\n").trim(),
    order: Number(filename.match(/^\d+/u)?.[0] ?? 99),
  };
}
