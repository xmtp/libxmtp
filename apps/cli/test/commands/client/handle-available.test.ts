import { beforeAll, describe, expect, it } from "vitest";

import {
  createIdentityWithHandle,
  createRegisteredIdentity,
  parseJsonOutput,
  randomHandleName,
  runWithIdentity,
} from "../../helpers.js";

interface AvailableOutput {
  name: string;
  handles: string[];
}

describe("client handle-available", () => {
  let user: Awaited<ReturnType<typeof createRegisteredIdentity>>;
  let owner: Awaited<ReturnType<typeof createIdentityWithHandle>>;
  let codeOwner: Awaited<ReturnType<typeof createIdentityWithHandle>>;

  beforeAll(async () => {
    [user, owner, codeOwner] = await Promise.all([
      createRegisteredIdentity(),
      createIdentityWithHandle(),
      createIdentityWithHandle(`${randomHandleName()}#0007`),
    ]);
  });

  it("lists a free bare name first, then free codes", async () => {
    const name = randomHandleName();

    const result = await runWithIdentity(user, [
      "client",
      "handle-available",
      name.toUpperCase(),
      "--json",
    ]);

    expect(result.exitCode).toBe(0);
    const output = parseJsonOutput<AvailableOutput>(result.stdout);
    expect(output.name).toBe(name.toUpperCase());
    expect(output.handles[0]).toBe(name);
    expect(output.handles.length).toBeGreaterThan(1);
    expect(new Set(output.handles).size).toBe(output.handles.length);
    for (const handle of output.handles.slice(1)) {
      expect(handle).toMatch(new RegExp(`^${name}#\\d{4}$`));
    }
  });

  it("leaves out a claimed bare name", async () => {
    const result = await runWithIdentity(user, [
      "client",
      "handle-available",
      owner.handle,
    ]);

    expect(result.exitCode).toBe(0);
    const lines = result.stdout.split("\n");
    expect(lines).not.toContain(owner.handle);
    expect(lines.length).toBeGreaterThan(0);
    for (const line of lines) {
      expect(line).toMatch(new RegExp(`^${owner.handle}#\\d{4}$`));
    }
  });

  it("checks only the given handle when it has a code", async () => {
    const handle = `${owner.handle}#0042`;

    const result = await runWithIdentity(user, [
      "client",
      "handle-available",
      handle,
      "--json",
    ]);

    expect(result.exitCode).toBe(0);
    expect(parseJsonOutput<AvailableOutput>(result.stdout).handles).toEqual([
      handle,
    ]);
  });

  it("says that a claimed handle with a code is taken", async () => {
    const json = await runWithIdentity(user, [
      "client",
      "handle-available",
      codeOwner.handle,
      "--json",
    ]);
    expect(json.exitCode).toBe(0);
    expect(parseJsonOutput<AvailableOutput>(json.stdout).handles).toEqual([]);

    const human = await runWithIdentity(user, [
      "client",
      "handle-available",
      codeOwner.handle,
    ]);
    expect(human.exitCode).toBe(0);
    expect(human.stdout).toBe(`${codeOwner.handle} is taken.`);
  });

  it("rejects text outside the handle grammar", async () => {
    const result = await runWithIdentity(user, [
      "client",
      "handle-available",
      "nick.eth",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("nick.eth is not a valid handle.");
  });
});
