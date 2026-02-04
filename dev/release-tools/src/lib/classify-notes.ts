export interface ReleaseNoteFrontmatter {
  sdk: string | null;
  previousReleaseTag: string | null;
}

export interface ClassifiedNote {
  sdk: string;
  filePath: string;
  previousReleaseTag: string;
}

export interface ClassifyResult {
  empty: ClassifiedNote[];
  content: ClassifiedNote[];
}

/**
 * Extract `sdk` and `previous_release_tag` from YAML frontmatter between `---` markers.
 */
export function parseFrontmatter(text: string): ReleaseNoteFrontmatter {
  const match = text.match(/^---\n([\s\S]*?)\n---/);
  if (!match) {
    return { sdk: null, previousReleaseTag: null };
  }

  const frontmatter = match[1];
  let sdk: string | null = null;
  let previousReleaseTag: string | null = null;

  for (const line of frontmatter.split("\n")) {
    const sdkMatch = line.match(/^sdk:\s*(.+)$/);
    if (sdkMatch) {
      const value = sdkMatch[1].trim();
      sdk = value === "null" ? null : value;
    }
    const tagMatch = line.match(/^previous_release_tag:\s*(.+)$/);
    if (tagMatch) {
      const value = tagMatch[1].trim();
      previousReleaseTag = value === "null" ? null : value;
    }
  }

  return { sdk, previousReleaseTag };
}

/**
 * Strip frontmatter, HTML comments, `#` headers, blank lines, and whitespace;
 * return `true` if nothing remains.
 */
export function isEmptyScaffold(text: string): boolean {
  // Strip frontmatter
  let body = text.replace(/^---\n[\s\S]*?\n---/, "");
  // Strip HTML comments
  body = body.replace(/<!--[\s\S]*?-->/g, "");
  // Strip lines that are headers
  body = body
    .split("\n")
    .filter((line) => !/^\s*#/.test(line))
    .join("\n");
  // Strip all whitespace
  body = body.replace(/\s/g, "");
  return body.length === 0;
}

/**
 * Classify an array of file entries into empty scaffolds and files with content.
 * Skips files with missing/null `sdk` or `previous_release_tag`.
 */
export function classifyNoteFiles(
  files: { path: string; content: string }[],
): ClassifyResult {
  const empty: ClassifiedNote[] = [];
  const content: ClassifiedNote[] = [];

  for (const file of files) {
    const fm = parseFrontmatter(file.content);
    if (!fm.sdk || !fm.previousReleaseTag) {
      continue;
    }

    const note: ClassifiedNote = {
      sdk: fm.sdk,
      filePath: file.path,
      previousReleaseTag: fm.previousReleaseTag,
    };

    if (isEmptyScaffold(file.content)) {
      empty.push(note);
    } else {
      content.push(note);
    }
  }

  return { empty, content };
}
