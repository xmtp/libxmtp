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

export function getCommitsBetween(
  cwd: string,
  sinceRef: string | null,
  untilRef: string
): string[] {
  const range = sinceRef ? `${sinceRef}..${untilRef}` : untilRef;
  const output = exec(
    `git log ${range} --oneline --no-decorate`,
    cwd
  );
  if (!output) return [];
  return output.split("\n").filter(Boolean);
}
