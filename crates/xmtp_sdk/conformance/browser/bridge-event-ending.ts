import { expect, it } from "vitest";

import {
  RemoteObject,
  endOwner,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/remote-object";
import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session";
import * as P from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import { EventStream as HostEventStream } from "../../../../target/sdk-generated/typescript-wasm/runtime/events/reader";
import { publicEventStream } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/events";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";
import { pair } from "./bridge-support";

class EventProjection extends P.ObjectProjection {
  liftMessage(): never {
    throw new Error("no message in these events");
  }
  lowerMessage(): never {
    throw new Error("no message in these events");
  }
}

class Reader extends RemoteObject {
  next(): Promise<B.ClientEvent | undefined> {
    return this.call("EventReader.next", []) as Promise<
      B.ClientEvent | undefined
    >;
  }
  async end(): Promise<void> {
    await this.call("EventReader.end", []);
  }
}

class ListenerOwner extends RemoteObject {
  async stopListener(): Promise<void> {
    await this.call("Client.stopListener", [1n]);
  }
}

async function fixture() {
  P.installProjection(new EventProjection());
  let stops = 0;
  let listenerStops = 0;
  const [main, worker] = pair();
  worker.onMessage((message) => {
    if (message.t === "hello") worker.postMessage({ t: "ready", epoch: 0 });
    if (message.t !== "call") return;
    const value =
      message.key === "EventReader.next"
        ? B.ClientEvent.ArchiveRestored.new({
            archiveRestored: { complete: true },
          })
        : undefined;
    if (message.key === "EventReader.end") stops++;
    if (message.key === "Client.stopListener") listenerStops++;
    worker.postMessage({ t: "return", id: message.id, value });
  });
  const session = new MainSession(main, 1, "event-return");
  session.setErrorDecoder((wire) => {
    if (wire.variant === "ClientClosed")
      return B.XmtpError.ClientClosed.new(
        (wire.details as B.ErrorDetails[])[0]!,
      );
    throw new Error(`unexpected bridge error ${wire.variant}`);
  });
  await session.ready();
  const owner = 1;
  const reader = new Reader(session, {
    h: 2,
    type: "EventReader",
    owner,
    epoch: 0,
  });
  return {
    session,
    owner,
    stream: publicEventStream(
      new HostEventStream(reader as unknown as B.EventReaderLike),
    ),
    stops: () => stops,
    listenerStops: () => listenerStops,
    listenerOwner: new ListenerOwner(session, {
      h: 1,
      type: "Client",
      owner,
      epoch: 0,
    }),
  };
}

const done = { done: true, value: undefined };

export function registerEventEndingTests(): void {
  for (const outcome of ["live", "success", "rollback", "death"] as const) {
    it(`event listener stop after owner close preserves ${outcome}`, async () => {
      const { session, owner, listenerOwner, listenerStops } = await fixture();
      try {
        if (outcome !== "live") session.fenceOwner(owner);
        let settled = false;
        const stopped = listenerOwner.stopListener().then(
          () => {
            settled = true;
            return undefined;
          },
          (error: unknown) => {
            settled = true;
            return error;
          },
        );
        await session.call("barrier", []);
        if (outcome !== "live") {
          expect(settled).toBe(false);
          expect(listenerStops()).toBe(0);
        }
        if (outcome === "success") endOwner(listenerOwner);
        else if (outcome === "rollback") session.unfenceOwner(owner);
        else if (outcome === "death") session.terminate();
        if (outcome === "death") {
          expect(await stopped).toBeInstanceOf(Error);
        } else {
          expect(await stopped).toBeUndefined();
          expect(listenerStops()).toBe(outcome === "success" ? 0 : 1);
          if (outcome === "success") {
            await expect(listenerOwner.stopListener()).resolves.toBeUndefined();
            await expect(listenerOwner.stopListener()).resolves.toBeUndefined();
            expect(listenerStops()).toBe(0);
          }
        }
      } finally {
        session.terminate();
      }
    });
  }

  // verifies: EVENT-054
  it("event return after owner close ends unused and delivered streams", async () => {
    for (const delivered of [false, true]) {
      const { session, owner, stream, stops } = await fixture();
      try {
        if (delivered) {
          // A for-await break calls return after the loop body ends its client.
          for await (const event of stream) {
            expect(event).toEqual({
              kind: "archive.restored",
              archive_restored: { complete: true },
            });
            session.fenceOwner(owner);
            session.closeOwner(owner, []);
            break;
          }
        } else {
          session.fenceOwner(owner);
          session.closeOwner(owner, []);
        }
        await expect(stream.return()).resolves.toEqual(done);
        await expect(stream.return()).resolves.toEqual(done);
        await expect(stream.next()).resolves.toEqual(done);
        expect(stops()).toBe(0);
      } finally {
        session.terminate();
      }
    }
  });

  it("event return after owner close keeps live cleanup idempotent", async () => {
    const { session, stream, stops } = await fixture();
    try {
      await expect(stream.return()).resolves.toEqual(done);
      await expect(stream.return()).resolves.toEqual(done);
      await expect(stream.next()).resolves.toEqual(done);
      expect(stops()).toBe(1);
    } finally {
      session.terminate();
    }
  });

  for (const outcome of ["success", "rollback", "death"] as const) {
    it(`event return after owner close waits for ${outcome}`, async () => {
      const { session, owner, stream, stops } = await fixture();
      try {
        session.fenceOwner(owner);
        let settled = false;
        const returned = stream.return().then(
          (value) => {
            settled = true;
            return { value };
          },
          (error: unknown) => {
            settled = true;
            return { error };
          },
        );
        // This round trip follows the stop call reaching the owner fence.
        await session.call("barrier", []);
        expect(stops()).toBe(0);
        expect(settled).toBe(false);
        if (outcome === "success") session.closeOwner(owner, []);
        else if (outcome === "rollback") session.unfenceOwner(owner);
        else session.terminate();
        if (outcome === "death") {
          const result = await returned;
          expect(result).toHaveProperty("error");
          if (!("error" in result)) throw new Error("worker failure was lost");
          expect(result.error).toBeInstanceOf(Error);
        } else {
          expect(await returned).toEqual({ value: done });
          expect(stops()).toBe(outcome === "rollback" ? 1 : 0);
        }
      } finally {
        session.terminate();
      }
    });
  }
}
