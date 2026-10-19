import { type Client, type ClientEvent } from "@xmtp/react-native-sdk";

import { assert, equal } from "./assertions";
import { deferred, Mailbox, within } from "./async";

// verifies: EVENT-009, EVENT-010, EVENT-014, EVENT-015, EVENT-020, EVENT-022,
// verifies: EVENT-030, EVENT-031, EVENT-033, EVENT-050, EVENT-051, EVENT-052, EVENT-053
export async function eventJourney(client: Client): Promise<string[]> {
  const filter = { kinds: ["consent.changed" as const] };
  const slow = new Mailbox<ClientEvent>();
  const fast = new Mailbox<ClientEvent>();
  const release = deferred<void>();
  const stopped = deferred<void>();
  let active = 0;
  let maximum = 0;
  let calls = 0;
  const marker = "f".repeat(64);
  const fastId = await client.startListener(filter, (event) => {
    fast.push(event);
  });
  const slowId = await client.startListener(filter, async (event) => {
    active++;
    maximum = Math.max(maximum, active);
    calls++;
    try {
      slow.push(event);
      if (calls === 1) {
        await release.promise;
        throw new Error("event callback rejection must be contained");
      }
      if (
        event.kind === "consent.changed" &&
        event.consent_changed.entity === marker
      ) {
        equal(
          await client.preferences.consentState({
            kind: "inbox",
            inboxId: marker,
          }),
          "allowed",
          "event state must be readable from a reentrant callback",
        );
        await client.stopListener(slowId);
        stopped.resolve();
      }
    } finally {
      active--;
    }
  });
  const write = async (id: string) => {
    await within(
      client.preferences.setConsentStates([
        { entity: { kind: "inbox", inboxId: id }, state: "allowed" },
      ]),
      "consent mutation while another callback is held",
    );
    const event = await fast.next("independent fast listener");
    assert(
      event.kind === "consent.changed",
      "fast listener must not lose events",
    );
    equal(
      event.consent_changed.entity,
      id,
      "native callback payload and emission order",
    );
  };
  try {
    await write((1).toString(16).padStart(64, "0"));
    await slow.next("first callback has entered JS");
    // Exactly 1030 admitted changes: one active + 1023 retained + six drops.
    for (let index = 2; index <= 1030; index++)
      await write(index.toString(16).padStart(64, "0"));
    equal(calls, 1, "held native listener must not admit concurrent callbacks");
    release.resolve();
    let lagged = 0;
    let sequence = 2;
    for (let index = 0; index < 1024; index++) {
      const event = await slow.next("retained event or overflow count");
      if (event.kind === "lagged") {
        lagged++;
        equal(
          event.lagged.discarded,
          6n,
          "queue bound must include the JS callback and every native transport queue",
        );
      } else {
        assert(
          event.kind === "consent.changed",
          "filter must run before queue admission",
        );
        equal(
          event.consent_changed.entity,
          sequence.toString(16).padStart(64, "0"),
          "overflow must drop new events and preserve retained admission order",
        );
        sequence++;
      }
    }
    equal(lagged, 1, "lagged must arrive even when absent from the filter");
    equal(
      sequence,
      1025,
      "exactly 1023 changes remain after the held callback",
    );
    equal(maximum, 1, "one callback at a time across JSI");
    await write(marker);
    await within(stopped.promise, "callback can stop its own native listener");
    const atStop = calls;
    await write("e".repeat(64));
    equal(calls, atStop, "stopped listener must not receive a later change");
    return [
      "native event queue counts active JS handoff, exact overflow/order, slow-listener isolation, rejection and reentrant stop",
    ];
  } finally {
    release.resolve();
    await client.stopListener(slowId);
    await client.stopListener(fastId);
  }
}

// verifies: EVENT-056
export async function eventClientIsolationJourney(
  first: Client,
  second: Client,
): Promise<string[]> {
  const left = await first.events({ kinds: ["consent.changed"] });
  const right = await second.events({ kinds: ["consent.changed"] });
  const write = (client: Client, suffix: string) =>
    client.preferences.setConsentStates([
      {
        entity: { kind: "inbox", inboxId: suffix.repeat(64) },
        state: "allowed",
      },
    ]);
  const read = async (events: typeof left, suffix: string) => {
    const item = await within(
      events.next(),
      "event on its owning native client",
    );
    assert(
      !item.done && item.value.kind === "consent.changed",
      "own client must emit consent change",
    );
    equal(
      item.value.consent_changed.entity,
      suffix.repeat(64),
      "native subscriptions must not mix separate clients sharing a database",
    );
  };
  try {
    await write(first, "a");
    await write(second, "b");
    await read(left, "a");
    await read(right, "b");
    await write(first, "c");
    await write(second, "d");
    await read(left, "c");
    await read(right, "d");
    return [
      "native event subscriptions isolate two client instances sharing one database",
    ];
  } finally {
    await left.return();
    await right.return();
  }
}

// verifies: EVENT-052, EVENT-053, EVENT-054
export async function eventShutdownJourney(
  client: Client,
  barrierClient: Client,
  closeClient: boolean,
): Promise<string[]> {
  const filter = { kinds: ["consent.changed" as const] };
  const entered = deferred<void>();
  const release = deferred<void>();
  const returned = deferred<void>();
  const forbidden = deferred<void>();
  const admitted = new Mailbox<ClientEvent>();
  let calls = 0;
  const observer = await client.startListener(filter, (event) =>
    admitted.push(event),
  );
  const held = await client.startListener(filter, async () => {
    calls++;
    if (calls > 1) forbidden.resolve();
    entered.resolve();
    await release.promise;
    returned.resolve();
  });
  const barrierEvents = new Mailbox<ClientEvent>();
  const barrierId = await barrierClient.startListener(filter, (event) =>
    barrierEvents.push(event),
  );
  const write = (owner: Client, index: number) =>
    owner.preferences.setConsentStates([
      {
        entity: {
          kind: "inbox",
          inboxId: index.toString(16).padStart(64, "0"),
        },
        state: "allowed",
      },
    ]);
  try {
    await write(client, closeClient ? 8001 : 7001);
    await admitted.next("first shutdown event admitted");
    await within(entered.promise, "shutdown callback held in JS");
    for (let index = 2; index <= 4; index++) {
      await write(client, (closeClient ? 8000 : 7000) + index);
      await admitted.next("native queued event observed independently");
    }
    equal(calls, 1, "shutdown starts with one held callback and queued events");
    await within(
      closeClient ? client.end() : client.stopListener(held),
      "native shutdown must complete while callback is held",
    );
    release.resolve();
    await within(returned.promise, "held JS callback returns after shutdown");
    // Prove the surviving client can still deliver. This callback is unordered
    // against the retired Rust listener and is not a native completion barrier.
    await write(barrierClient, closeClient ? 9002 : 9001);
    await barrierEvents.next("native callback barrier after shutdown");
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        forbidden.promise.then(() => {
          throw new Error(
            "shutdown must discard queued events before any further JS handoff",
          );
        }),
        new Promise<void>((resolve) => {
          timer = setTimeout(resolve, 2000);
        }),
      ]);
    } finally {
      if (timer !== undefined) clearTimeout(timer);
    }
    equal(
      calls,
      1,
      "shutdown must discard queued events before any further JS handoff",
    );
    return [
      closeClient
        ? "client end while callback held has no queued handoff in bounded observation window"
        : "listener stop while callback held has no queued handoff in bounded observation window",
    ];
  } finally {
    release.resolve();
    await barrierClient.stopListener(barrierId);
    if (!closeClient) {
      await client.stopListener(held);
      await client.stopListener(observer);
    }
  }
}
