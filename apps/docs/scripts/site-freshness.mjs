import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstat,
  readFile,
  readdir,
  readlink,
  rm,
  writeFile,
} from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const STAMP = "site-inputs.json";

async function hashFiles(root, paths) {
  const hash = createHash("sha256");
  for (const path of paths.sort()) {
    hash.update(path).update("\0");
    try {
      const file = join(root, path);
      const entry = await lstat(file);
      const content = entry.isSymbolicLink()
        ? await readlink(file)
        : await readFile(file);
      hash.update(entry.isSymbolicLink() ? "symlink\0" : "file\0");
      hash.update(createHash("sha256").update(content).digest());
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      hash.update("deleted");
    }
    hash.update("\0");
  }
  return hash.digest("hex");
}

export async function sourceHash(repositoryRoot) {
  // Include tracked files, working changes, and new nonignored files.
  // Hash all source files so a new build input cannot bypass this check.
  const paths = [
    ...new Set(
      execFileSync(
        "git",
        ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        {
          cwd: repositoryRoot,
          encoding: "utf8",
        },
      )
        .split("\0")
        .filter(Boolean),
    ),
  ];
  if (paths.length === 0) throw new Error("Site source inputs are missing.");
  return hashFiles(repositoryRoot, paths);
}

async function outputHash(distRoot) {
  const paths = [];
  async function visit(directory = "") {
    for (const entry of await readdir(join(distRoot, directory), {
      withFileTypes: true,
    })) {
      const path = join(directory, entry.name);
      if (path === STAMP) continue;
      if (entry.isDirectory()) await visit(path);
      else paths.push(path);
    }
  }
  const index = await readFile(join(distRoot, "index.html"));
  if (index.length === 0) throw new Error("Site entrypoint is empty.");
  await visit();
  return hashFiles(distRoot, paths);
}

export async function buildSite({ repositoryRoot, distRoot, build }) {
  await rm(join(distRoot, STAMP), { force: true });
  const source = await sourceHash(repositoryRoot);
  await build();
  if (source !== (await sourceHash(repositoryRoot))) {
    throw new Error("Site inputs changed during the build. Build again.");
  }
  await writeFile(
    join(distRoot, STAMP),
    JSON.stringify({ version: 1, source, output: await outputHash(distRoot) }) +
      "\n",
  );
}

export async function checkFreshness({ repositoryRoot, distRoot }) {
  let stamp;
  try {
    stamp = JSON.parse(await readFile(join(distRoot, STAMP), "utf8"));
  } catch {
    throw new Error(
      "Site build stamp is missing or invalid. Follow docs/AGENTS.md.",
    );
  }
  if (
    stamp.version !== 1 ||
    stamp.source !== (await sourceHash(repositoryRoot)) ||
    stamp.output !== (await outputHash(distRoot))
  ) {
    throw new Error(
      "Site build is stale. Build current inputs or download matching artifacts. Follow docs/AGENTS.md.",
    );
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const repositoryRoot = resolve(import.meta.dirname, "../../..");
  await checkFreshness({
    repositoryRoot,
    distRoot: join(repositoryRoot, "apps/docs/dist"),
  });
  console.log("OK: site build matches current inputs");
}
