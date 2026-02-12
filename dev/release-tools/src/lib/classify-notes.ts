import { parse } from "smol-toml";

export interface ReleaseNoteFrontmatter {
  sdk: string | null;
  previousReleaseVersion: string | null;
  previousReleaseTag: string | null;
}

export interface ClassifiedNote {
  sdk: string;
  filePath: string;
  previousReleaseVersion: string | null;
  previousReleaseTag: string | null;
}

export interface ClassifyResult {
  empty: ClassifiedNote[];
  content: ClassifiedNote[];
}

/**
 * Extract TOML frontmatter between `---` markers and parse it.
 * Returns nulls for missing/invalid frontmatter or non-string values.
 */
export function parseFrontmatter(text: string): ReleaseNoteFrontmatter {
  const match = text.match(/^---\n([\s\S]*?)\n---/);
  if (!match) {
    return { sdk: null, previousReleaseVersion: null, previousReleaseTag: null };
  }

  let parsed: Record<string, unknown>;
  try {
    parsed = parse(match[1]);
  } catch {
    return { sdk: null, previousReleaseVersion: null, previousReleaseTag: null };
  }

  const sdk = typeof parsed.sdk === "string" ? parsed.sdk : null;
  const previousReleaseVersion =
    typeof parsed.previous_release_version === "string"
      ? parsed.previous_release_version
      : null;
  const previousReleaseTag =
    typeof parsed.previous_release_tag === "string"
      ? parsed.previous_release_tag
      : null;

  return { sdk, previousReleaseVersion, previousReleaseTag };
}

/**
 * Check if a release note file is an empty scaffold (no real content).
 * Iterates lines, skipping frontmatter, blank lines, headers, and HTML comments.
 * Returns early when real content is found.
 */
export function isEmptyScaffold(text: string): boolean {
  let inFrontmatter = false;

  for (const line of text.split("\n")) {
    const trimmed = line.trim();

    if (trimmed === "---") {
      inFrontmatter = !inFrontmatter;
      continue;
    }
    if (inFrontmatter) continue;
    if (trimmed === "") continue;
    if (trimmed.startsWith("#")) continue;
    if (trimmed.startsWith("<!--")) continue;
    return false;
  }

  return true;
}

/**
 * Classify an array of file entries into empty scaffolds and files with content.
 * Skips files missing `sdk` or both `previous_release_version` and `previous_release_tag`.
 */
export function classifyNoteFiles(
  files: { path: string; content: string }[],
): ClassifyResult {
  const empty: ClassifiedNote[] = [];
  const content: ClassifiedNote[] = [];

  for (const file of files) {
    const fm = parseFrontmatter(file.content);
    if (!fm.sdk || (!fm.previousReleaseVersion && !fm.previousReleaseTag)) {
      continue;
    }

    const note: ClassifiedNote = {
      sdk: fm.sdk,
      filePath: file.path,
      previousReleaseVersion: fm.previousReleaseVersion,
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
