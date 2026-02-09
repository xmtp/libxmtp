import { exec } from "./exec";

export function listTags(cwd: string): string[] {
  const output = exec("git tag --list", cwd);
  if (!output) return [];
  return output.split("\n").filter(Boolean);
}

export function getShortSha(cwd: string, ref = "HEAD"): string {
  return exec(`git rev-parse --short=7 ${ref}`, cwd);
}

export function getRepoRoot(cwd?: string): string {
  try {
    return exec("git rev-parse --show-toplevel", cwd ?? process.cwd());
  } catch {
    return process.cwd();
  }
}

export function getCommitsBetween(
  cwd: string,
  sinceRef: string | null,
  untilRef: string,
): string[] {
  const range = sinceRef ? `${sinceRef}..${untilRef}` : untilRef;
  const output = exec(`git log ${range} --oneline --no-decorate`, cwd);
  if (!output) return [];
  return output.split("\n").filter(Boolean);
}

export function createTag(
  cwd: string,
  tag: string,
  ignoreIfExists = false,
): void {
  const existing = exec(`git tag --list ${tag}`, cwd);
  if (existing) {
    if (ignoreIfExists) {
      console.log(`Tag ${tag} already exists locally, skipping creation`);
      return;
    }
    throw new Error(`Tag ${tag} already exists`);
  }
  exec(`git tag ${tag}`, cwd);
}

export function pushTag(
  cwd: string,
  tag: string,
  pushBranch: boolean,
  ignoreIfExists = false,
): void {
  try {
    if (pushBranch) {
      exec(`git push origin HEAD ${tag}`, cwd);
    } else {
      exec(`git push origin ${tag}`, cwd);
    }
  } catch (err) {
    if (ignoreIfExists) {
      const remoteTag = exec(
        `git ls-remote --tags origin refs/tags/${tag}`,
        cwd,
      );
      if (remoteTag.includes(tag)) {
        console.log(`Tag ${tag} already exists on remote, skipping push`);
        if (pushBranch) {
          exec(`git push origin HEAD`, cwd);
        }
        return;
      }
    }
    throw err;
  }
}
