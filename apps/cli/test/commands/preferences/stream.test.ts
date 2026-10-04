import { describe, expect, it, vi } from "vitest";

import PreferencesStream from "../../../src/commands/preferences/stream.js";
import { createRegisteredIdentity, runWithIdentity } from "../../helpers.js";

describe("preferences stream", () => {
  it("keeps the CLI entity types for both consent event kinds", async () => {
    const client = {
      events: vi.fn(async () =>
        (async function* () {
          yield {
            kind: "consent.changed",
            consent_changed: {
              entity_kind: "inbox",
              entity: "inbox-1",
              state: "allowed",
            },
          };
          yield {
            kind: "consent.changed",
            consent_changed: {
              entity_kind: "conversation",
              entity: "group-1",
              state: "denied",
            },
          };
        })(),
      ),
    };
    const output = vi.fn();
    const command = Object.assign(Object.create(PreferencesStream.prototype), {
      parse: vi.fn(async () => ({ flags: { count: 2 } })),
      initClient: vi.fn(async () => client),
      streamOutput: output,
    }) as PreferencesStream;

    await command.run();

    expect(output.mock.calls.map(([value]) => value.updates[0])).toEqual([
      {
        type: "ConsentUpdate",
        entityType: "inbox_id",
        entity: "inbox-1",
        state: "allowed",
      },
      {
        type: "ConsentUpdate",
        entityType: "conversation_id",
        entity: "group-1",
        state: "denied",
      },
    ]);
  });

  it("reports discarded events without claiming an HMAC update", async () => {
    const hmacKeys = vi.fn(async () => []);
    const client = {
      events: vi.fn(async () =>
        (async function* () {
          yield { kind: "lagged", lagged: { discarded: 3n } };
          yield { kind: "hmac_keys.updated", hmac_keys_updated: {} };
        })(),
      ),
      conversations: { hmacKeys },
    };
    const output = vi.fn();
    const command = Object.assign(Object.create(PreferencesStream.prototype), {
      parse: vi.fn(async () => ({ flags: { count: 1 } })),
      initClient: vi.fn(async () => client),
      streamOutput: output,
    }) as PreferencesStream;

    await command.run();

    expect(output).toHaveBeenCalledTimes(2);
    expect(output.mock.calls[0]![0]).toMatchObject({
      warning: { type: "Lagged", discarded: 3n },
    });
    expect(output.mock.calls[1]![0]).toMatchObject({
      updates: [{ type: "HmacKeyUpdate" }],
    });
    expect(hmacKeys).toHaveBeenCalledOnce();
  });

  it("streams with timeout and exits cleanly", async () => {
    const user = await createRegisteredIdentity();

    const result = await runWithIdentity(
      user,
      ["preferences", "stream", "--timeout", "2", "--json"],
      { timeout: 10000 },
    );

    // Should exit cleanly after timeout
    expect(result.exitCode, result.stderr).toBe(0);
  });
});
