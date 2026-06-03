import { readFileSync } from "node:fs";

/**
 * Read a CLI `--input` value: `-` reads stdin (fd 0), anything else is a path.
 */
export function readInput(input: string): string {
  return input === "-"
    ? readFileSync(0, "utf-8")
    : readFileSync(input, "utf-8");
}
