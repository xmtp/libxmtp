import { execSync } from "node:child_process";

/** Run a command and return its stdout (trimmed). */
export function exec(cmd: string, cwd: string): string {
  return execSync(cmd, { cwd, encoding: "utf-8" }).trim();
}

/** Run a command silently, suppressing all output. */
export function execSilent(cmd: string, cwd: string): void {
  execSync(cmd, { cwd, stdio: "pipe" });
}
