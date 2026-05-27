import fs from "node:fs";
import {
  applyEdits,
  modify,
  parse as parseJsonc,
  printParseErrorCode,
  type ParseError,
} from "jsonc-parser";

const IMAGE_KEY = "image";
const BUILD_KEY = "build";

/**
 * Rewrite `.devcontainer/devcontainer.json` so it pins the dev container to a
 * specific image reference.
 *
 * The file is JSONC (JSON with comments). We use Microsoft's `jsonc-parser` —
 * the same library VS Code uses to read devcontainer.json — to apply surgical
 * edits that preserve comments, whitespace, and any unrelated keys exactly as
 * they were in the source file.
 *
 * Two cases are handled:
 *
 *   1. The file already has an `"image"` key: the value is replaced in place.
 *   2. The file has a `"build"` block: a new `"image"` key is inserted at the
 *      same position and the `"build"` key is removed, so the resulting file
 *      has `"image"` sitting exactly where `"build"` used to.
 */
export function setDevcontainerImage(
  devcontainerJsonPath: string,
  image: string,
): void {
  const source = fs.readFileSync(devcontainerJsonPath, "utf-8");

  const errors: ParseError[] = [];
  const parsed: unknown = parseJsonc(source, errors);
  if (errors.length > 0 || !isPlainObject(parsed)) {
    const detail = errors.map((e) => printParseErrorCode(e.error)).join(", ");
    throw new Error(
      `Failed to parse ${devcontainerJsonPath}${detail ? `: ${detail}` : ""}`,
    );
  }

  const hasImage = IMAGE_KEY in parsed;
  const hasBuild = BUILD_KEY in parsed;
  if (!hasImage && !hasBuild) {
    throw new Error(
      `Could not find '${IMAGE_KEY}' or '${BUILD_KEY}' key in ${devcontainerJsonPath}`,
    );
  }

  const formattingOptions = {
    insertSpaces: true,
    tabSize: detectTabSize(source),
  };

  let result = source;
  const edit = (path: (string | number)[], value: unknown, extra = {}): void => {
    result = applyEdits(
      result,
      modify(result, path, value, { formattingOptions, ...extra }),
    );
  };

  if (hasImage) {
    edit([IMAGE_KEY], image);
  } else {
    edit([IMAGE_KEY], image, {
      getInsertionIndex: (properties: string[]) => properties.indexOf(BUILD_KEY),
    });
    edit([BUILD_KEY], undefined);
  }

  fs.writeFileSync(devcontainerJsonPath, result);
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Detect the indent width used in an existing JSON source by looking at the
 * first indented `"key":` line. Falls back to 2 spaces.
 */
function detectTabSize(source: string): number {
  const match = source.match(/\n( +)"/);
  return match && match[1].length > 0 ? match[1].length : 2;
}
