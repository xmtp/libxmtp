import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import { dispatchGenerated } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/dispatch.gen";
import { uniffiInitAsync } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import {
  browserPoolLocks,
  WorkerHost,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/worker/host";

const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    self.postMessage(message, { transfer });
  },
  onMessage(handler) {
    self.addEventListener("message", (event: MessageEvent<WireMessage>) =>
      handler(event.data),
    );
  },
  onExit() {},
  close() {
    self.close();
  },
};

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
let generation = 0;
let gate:
  | {
      generation: number;
      armed: boolean;
      released: boolean;
      held: ReturnType<typeof latch>;
      aborted: ReturnType<typeof latch>;
      release: ReturnType<typeof latch>;
    }
  | undefined;
// A held Client.end that fails when released, to model an end that does not
// complete.
let endHold: ReturnType<typeof latch> | undefined;
let endEntered: ReturnType<typeof latch> | undefined;
let nextCalls = 0;
let endCalls = 0;
let endCompletions = 0;

new WorkerHost(
  endpoint,
  PROTOCOL_VERSION,
  CONTRACT_HASH,
  () =>
    uniffiInitAsync(
      new URL(
        "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/xmtp_sdk.wasm",
        import.meta.url,
      ),
    ),
  async (key, args, context) => {
    if (key === "__f3ArmNext") {
      if (gate && !gate.released) throw new Error("gate still active");
      gate = {
        generation: ++generation,
        armed: true,
        released: false,
        held: latch(),
        aborted: latch(),
        release: latch(),
      };
      return generation;
    }
    if (key === "__f3WaitHeld") {
      if (!gate) throw new Error("gate absent");
      await gate.held.promise;
      return gate.generation;
    }
    if (key === "__f3WaitAbort") {
      if (!gate) throw new Error("gate absent");
      await gate.aborted.promise;
      return gate.generation;
    }
    if (key === "__f3Release") {
      if (!gate || gate.released || args[0] !== gate.generation)
        throw new Error("stale release");
      gate.released = true;
      gate.release.resolve();
      return undefined;
    }
    if (key === "__f3Counts") return { nextCalls, endCalls, endCompletions };
    if (key === "__f3HoldEnd") {
      endHold = latch();
      endEntered = latch();
      return undefined;
    }
    if (key === "__f3WaitEnd") {
      await endEntered?.promise;
      return undefined;
    }
    if (key === "__f3FailEnd") {
      endHold?.resolve();
      return undefined;
    }
    if (key === "Client.end" && endHold) {
      const hold = endHold;
      endEntered?.resolve();
      await hold.promise;
      endHold = undefined;
      endEntered = undefined;
      throw new Error("end failed");
    }
    if (key === "MessageReader.next") nextCalls++;
    if (key === "MessageReader.end") endCalls++;
    const result = await dispatchGenerated(key, args, context);
    if (key === "MessageReader.end") endCompletions++;
    if (key === "MessageReader.next" && gate?.armed) {
      const held = gate;
      held.armed = false;
      if (context.signal.aborted) held.aborted.resolve();
      else
        context.signal.addEventListener("abort", held.aborted.resolve, {
          once: true,
        });
      held.held.resolve();
      await held.release.promise;
    }
    return result;
  },
  browserPoolLocks(),
);
