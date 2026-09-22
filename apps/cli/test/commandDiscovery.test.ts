import { readdirSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { COMMANDS } from "@/commands";
import CustomHelp from "@/help";
import * as entry from "@/index";

describe("bundled command discovery", () => {
  it("registers every command file with its original command ID", () => {
    const ids = readdirSync(new URL("../src/commands/", import.meta.url), {
      encoding: "utf8",
      recursive: true,
    })
      .filter((file) => file.endsWith(".ts"))
      .map((file) => file.replace(/\.ts$/, "").replaceAll(/[\\/]/g, ":"))
      .sort((left, right) => left.localeCompare(right));

    expect(
      Object.keys(COMMANDS).sort((left, right) => left.localeCompare(right)),
    ).toEqual(ids);
    for (const command of Object.values(COMMANDS)) {
      expect(typeof command.run).toBe("function");
    }
  });

  it("exports the command table and custom help from the bundle entry", () => {
    expect(entry.COMMANDS).toBe(COMMANDS);
    expect(entry.CustomHelp).toBe(CustomHelp);
    expect(typeof entry.run).toBe("function");
  });
});
