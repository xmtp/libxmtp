export function prepareDocument(raw, filename, { spec = false } = {}) {
  const lines = raw.split(/\r?\n/u);
  let inReview = false;
  let fence;
  const kept = [];
  for (const line of lines) {
    const marker = line.match(/^\s{0,3}(`{3,}|~{3,})/u)?.[1];
    if (marker) {
      if (!fence) fence = marker;
      else if (marker[0] === fence[0] && marker.length >= fence.length)
        fence = undefined;
    }
    if (spec && !fence && /^##\s+/u.test(line)) {
      inReview = /^##\s+Review (?:record|log)\s*$/iu.test(line);
    }
    if (!inReview && !(spec && !fence && /^Status:/u.test(line)))
      kept.push(line);
  }
  const titleIndex = kept.findIndex((line) => /^#\s+/u.test(line));
  if (titleIndex < 0) throw new Error(`Missing document title: ${filename}`);
  const title = kept.splice(titleIndex, 1)[0].replace(/^#\s+/u, "");
  return {
    title,
    body: kept.join("\n").trim(),
    order: Number(filename.match(/^\d+/u)?.[0] ?? 99),
  };
}
