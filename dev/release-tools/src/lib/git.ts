import { execSync } from "node:child_process";

function exec(cmd: string, cwd: string): string {
  return execSync(cmd, { cwd, encoding: "utf-8" }).trim();
}

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

export function createTag(cwd: string, tag: string): void {
  exec(`git tag ${tag}`, cwd);
}

export function pushTag(cwd: string, tag: string, pushBranch: boolean): void {
  if (pushBranch) {
    exec(`git push origin HEAD ${tag}`, cwd);
  } else {
    exec(`git push origin ${tag}`, cwd);
  }
}
