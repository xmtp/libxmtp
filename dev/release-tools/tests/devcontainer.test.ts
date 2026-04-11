import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { parse as parseJsonc } from "jsonc-parser";
import { setDevcontainerImage } from "../src/lib/devcontainer";

const BUILD_SHAPE = `// For format details, see https://aka.ms/devcontainer.json.
{
    "name": "libxmtp (Nix)",
    "build": {
        "dockerfile": "Dockerfile"
    },
    "runArgs": ["--ulimit", "stack=-1:-1"],
    "workspaceFolder": "/workspaces/libxmtp",
    "remoteUser": "vscode"
}
`;

const IMAGE_SHAPE = `// For format details, see https://aka.ms/devcontainer.json.
{
    "name": "libxmtp (Nix)",
    "image": "ghcr.io/xmtp/libxmtp-devcontainer@sha256:oldoldoldoldoldoldoldoldoldoldoldoldoldoldoldoldoldoldoldoldolol",
    "runArgs": ["--ulimit", "stack=-1:-1"],
    "workspaceFolder": "/workspaces/libxmtp",
    "remoteUser": "vscode"
}
`;

const NEW_IMAGE =
  "ghcr.io/xmtp/libxmtp-devcontainer@sha256:newnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewn";

describe("setDevcontainerImage", () => {
  let tmpDir: string;
  let jsonPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-devctr-"));
    jsonPath = path.join(tmpDir, "devcontainer.json");
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("converts a build block to a pinned image reference", () => {
    fs.writeFileSync(jsonPath, BUILD_SHAPE);

    setDevcontainerImage(jsonPath, NEW_IMAGE);

    const updated = fs.readFileSync(jsonPath, "utf-8");
    const parsed = parseJsonc(updated) as Record<string, unknown>;
    expect(parsed.image).toBe(NEW_IMAGE);
    expect(parsed.build).toBeUndefined();
    expect(updated).not.toContain('"dockerfile"');
  });

  it("updates an existing image reference in place", () => {
    fs.writeFileSync(jsonPath, IMAGE_SHAPE);

    setDevcontainerImage(jsonPath, NEW_IMAGE);

    const updated = fs.readFileSync(jsonPath, "utf-8");
    const parsed = parseJsonc(updated) as Record<string, unknown>;
    expect(parsed.image).toBe(NEW_IMAGE);
    expect(updated.match(/"image":/g)).toHaveLength(1);
    expect(updated).not.toContain("oldoldoldold");
  });

  it("inserts image between name and runArgs where build used to be", () => {
    fs.writeFileSync(jsonPath, BUILD_SHAPE);

    setDevcontainerImage(jsonPath, NEW_IMAGE);

    const keys = Object.keys(
      parseJsonc(fs.readFileSync(jsonPath, "utf-8")) as Record<string, unknown>,
    );
    expect(keys.indexOf("image")).toBeGreaterThan(keys.indexOf("name"));
    expect(keys.indexOf("image")).toBeLessThan(keys.indexOf("runArgs"));
    expect(keys).not.toContain("build");
  });

  it("preserves the leading JSONC comment", () => {
    fs.writeFileSync(jsonPath, BUILD_SHAPE);

    setDevcontainerImage(jsonPath, NEW_IMAGE);

    const updated = fs.readFileSync(jsonPath, "utf-8");
    expect(updated.startsWith("// For format details")).toBe(true);
  });

  it("preserves the original indent width", () => {
    fs.writeFileSync(jsonPath, BUILD_SHAPE);

    setDevcontainerImage(jsonPath, NEW_IMAGE);

    const updated = fs.readFileSync(jsonPath, "utf-8");
    expect(updated).toContain('    "name": "libxmtp (Nix)"');
    expect(updated).toContain(`    "image": "${NEW_IMAGE}"`);
  });

  it("preserves unrelated keys", () => {
    fs.writeFileSync(jsonPath, BUILD_SHAPE);

    setDevcontainerImage(jsonPath, NEW_IMAGE);

    const parsed = parseJsonc(
      fs.readFileSync(jsonPath, "utf-8"),
    ) as Record<string, unknown>;
    expect(parsed.name).toBe("libxmtp (Nix)");
    expect(parsed.runArgs).toEqual(["--ulimit", "stack=-1:-1"]);
    expect(parsed.workspaceFolder).toBe("/workspaces/libxmtp");
    expect(parsed.remoteUser).toBe("vscode");
  });

  it("throws when neither build nor image is present", () => {
    fs.writeFileSync(
      jsonPath,
      `{
    "name": "libxmtp (Nix)",
    "workspaceFolder": "/workspaces/libxmtp"
}
`,
    );

    expect(() => setDevcontainerImage(jsonPath, NEW_IMAGE)).toThrow(
      /Could not find 'image' or 'build' key/,
    );
  });

  it("throws on malformed JSONC", () => {
    fs.writeFileSync(jsonPath, `{ "name": "oops" `);

    expect(() => setDevcontainerImage(jsonPath, NEW_IMAGE)).toThrow(
      /Failed to parse/,
    );
  });
});
