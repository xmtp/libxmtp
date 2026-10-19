import { type Client, type ClientEvent } from "@xmtp/react-native-sdk";

import { assert, equal } from "./assertions";
import { deferred, Mailbox, within } from "./async";

/**
 * One native event reaches a JS listener, which calls back into the SDK and
 * stops itself. crates/xmtp_sdk/src/tests/event_listeners.rs owns the queue
 * bound, ordering, failure and shutdown rules.
 */
// verifies: EVENT-010, EVENT-052, EVENT-053
export async function listenerJourney(client: Client): Promise<string[]> {
  const filter = { kinds: ["consent.changed" as const] };
  const first = "a".repeat(64);
  const later = "b".repeat(64);
  const observed = new Mailbox<ClientEvent>();
  const stopped = deferred<unknown>();
  let calls = 0;
  const observerId = await client.startListener(filter, (event) => {
    observed.push(event);
  });
  const listenerId = await client.startListener(filter, async (event) => {
    calls++;
    try {
      assert(
        event.kind === "consent.changed" &&
          event.consent_changed.entity === first,
        "JS listener must receive the native consent change",
      );
      equal(
        await client.preferences.consentState({
          kind: "inbox",
          inboxId: first,
        }),
        "allowed",
        "listener must read the change through a re-entrant SDK call",
      );
      await client.stopListener(listenerId);
      stopped.resolve(undefined);
    } catch (error) {
      stopped.resolve(error);
    }
  });
  const write = (inboxId: string) =>
    within(
      client.preferences.setConsentStates([
        { entity: { kind: "inbox", inboxId }, state: "allowed" },
      ]),
      "consent change",
    );
  const observe = async (inboxId: string) => {
    const event = await observed.next("observer listener");
    assert(
      event.kind === "consent.changed" &&
        event.consent_changed.entity === inboxId,
      "observer listener must receive each change in order",
    );
  };
  try {
    await write(first);
    const failure = await within(
      stopped.promise,
      "listener stops itself from its callback",
    );
    if (failure !== undefined) throw failure;
    await observe(first);
    await write(later);
    await observe(later);
    equal(calls, 1, "a stopped listener must not receive a later change");
    return [
      "native consent event reaches a JS listener that re-enters the SDK and stops itself; a later change reaches only the observer",
    ];
  } finally {
    await client.stopListener(listenerId);
    await client.stopListener(observerId);
  }
}
