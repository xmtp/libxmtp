import { waitForLog } from "../../../../apps/xmtp_sdk_bindgen/runtime-tests/ts/logging-wait.js";
import * as sdk from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
import { loggingInWorker } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/package-session.gen";
import { loggingContract } from "./logging-contract.js";

type ConfigurationPacket = {
  otel?: { serviceName: string };
  resourceAttributes?: Map<string, string>;
};
const configurationPackets: ConfigurationPacket[] = [];
let terminated = 0;
let notifyTermination: (() => void) | undefined;
const OriginalWorker = globalThis.Worker;
globalThis.Worker = class extends OriginalWorker {
  override postMessage(
    message: unknown,
    options?: Transferable[] | StructuredSerializeOptions,
  ): void {
    const packet = message as {
      t?: string;
      key?: string;
      args?: ConfigurationPacket[];
    };
    if (packet.t === "call" && packet.key === "initLogging")
      configurationPackets.push(structuredClone(packet.args![0]));
    if (Array.isArray(options)) super.postMessage(message, options);
    else super.postMessage(message, options);
  }
  override terminate(): void {
    terminated++;
    super.terminate();
    notifyTermination?.();
  }
};

function check(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

// verifies: LOG-002, LOG-003, LOG-004, LOG-007, LOG-008, LOG-009, LOG-012, LOG-013
export async function managedQueue(): Promise<void> {
  const admin = await sdk.Storage.admin();
  await sdk.initLogging({ level: "error" });
  const emit = async (count: number): Promise<void> => {
    await loggingInWorker((session) =>
      session.call("sdkConformanceEmit", [count]),
    );
  };
  try {
    await loggingContract({ setLogSink: sdk.setLogSink, emit });
    let enteredFirst!: () => void;
    let enteredSecond!: () => void;
    let releaseFirst!: () => void;
    let releaseSecond!: () => void;
    const first = new Promise<void>((resolve) => {
      enteredFirst = resolve;
    });
    const second = new Promise<void>((resolve) => {
      enteredSecond = resolve;
    });
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const secondGate = new Promise<void>((resolve) => {
      releaseSecond = resolve;
    });
    let calls = 0;
    await sdk.setLogSink({
      async log(record) {
        calls++;
        check(
          record.fields.get("sequence") === String(calls - 1),
          "Rust queue order changed",
        );
        if (calls === 1) {
          enteredFirst();
          await firstGate;
        } else if (calls === 2) {
          enteredSecond();
          await secondGate;
        } else throw new Error("unexpected queued record");
      },
    });
    await waitForLog(emit(2), "Rust emit waited for the held app callback");
    await waitForLog(first, "first Rust record was not delivered");
    await waitForLog(admin.end(), "admin end waited for log drain");
    check(terminated === 0, "worker retired during the first callback");
    releaseFirst();
    await waitForLog(second, "worker lost the queued Rust successor");
    check(terminated === 0, "worker retired during the successor callback");
    const retired = new Promise<void>((resolve) => {
      notifyTermination = resolve;
    });
    releaseSecond();
    await waitForLog(
      retired,
      "worker did not retire after the Rust queue drained",
    );
    check(
      calls === 2 && terminated === 1,
      "managed worker delivery or retirement changed",
    );
  } finally {
    notifyTermination = undefined;
    await admin.end();
  }
}

// verifies: LOG-002, LOG-007
export async function managedRestart(): Promise<void> {
  const before = terminated;
  let calls = 0;
  let delivered: (() => void) | undefined;
  const emit = () =>
    loggingInWorker((session) => session.call("sdkConformanceEmit", [1]));
  async function retire(
    admin: Awaited<ReturnType<typeof sdk.Storage.admin>>,
  ): Promise<void> {
    const retired = new Promise<void>((resolve) => {
      notifyTermination = resolve;
    });
    await admin.end();
    await waitForLog(retired, "worker did not retire with its sink retained");
    notifyTermination = undefined;
  }
  let admin = await sdk.Storage.admin();
  try {
    const options = {
      level: "error" as sdk.LogLevel,
      helper: () => {},
      resourceAttributes: new Map([["source", "accepted"]]),
      otel: { endpoint: "http://127.0.0.1:9", serviceName: "accepted" },
    };
    await sdk.initLogging(options);
    options.level = "off";
    options.resourceAttributes.set("source", "mutated");
    options.otel.serviceName = "mutated";
    await sdk.setLogSink({
      log() {
        calls++;
        delivered?.();
        return Promise.resolve();
      },
    });
    for (let index = 0; index < 2; index++) {
      const received = new Promise<void>((resolve) => {
        delivered = resolve;
      });
      await emit();
      await waitForLog(
        received,
        "accepted sink was lost across worker retirement",
      );
      check(
        calls === index + 1,
        "restored sink did not receive one Rust record",
      );
      await retire(admin);
      admin = await sdk.Storage.admin();
      const restored = configurationPackets.at(-1);
      check(
        restored?.resourceAttributes?.get("source") === "accepted",
        "restart retained a caller-mutated map",
      );
      check(
        restored?.otel?.serviceName === "accepted",
        "restart retained a caller-mutated nested record",
      );
    }
    await sdk.setLogSink();
    await retire(admin);
    admin = await sdk.Storage.admin();
    await emit();
    await retire(admin);
    check(calls === 2, "cleared sink returned in a replacement worker");
    check(terminated === before + 4, "a sink kept a retired worker alive");
  } finally {
    delivered = undefined;
    notifyTermination = undefined;
    await admin.end();
  }
}

// verifies: LOG-002, LOG-004, LOG-008
export async function managedFailureRestart(): Promise<void> {
  let active = 0;
  let maxActive = 0;
  let calls = 0;
  let entered: (() => void) | undefined;
  let gate = Promise.resolve();
  let release: (() => void) | undefined;
  const emit = () =>
    loggingInWorker((session) => session.call("sdkConformanceEmit", [1]));
  let admin = await sdk.Storage.admin();
  await sdk.initLogging({ level: "error" });
  await sdk.setLogSink({
    async log() {
      active++;
      maxActive = Math.max(maxActive, active);
      calls++;
      entered?.();
      try {
        await gate;
        // A callback can use the replacement worker without waiting on itself.
        const owner = await sdk.Storage.admin();
        await owner.end();
      } finally {
        active--;
      }
    },
  });
  try {
    for (let index = 0; index < 2; index++) {
      gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      const first = new Promise<void>((resolve) => {
        entered = resolve;
      });
      await emit();
      await waitForLog(first, "held app callback was not entered");
      const failed = await loggingInWorker((session) =>
        session.call("bridgeTestPanic", []),
      ).then(
        () => false,
        () => true,
      );
      check(failed, "worker panic did not fail");
      admin = await waitForLog(
        sdk.Storage.admin(),
        "replacement creation waited for log drain",
      );
      const second = new Promise<void>((resolve) => {
        entered = resolve;
      });
      await waitForLog(emit(), "replacement emit waited for log drain");
      await waitForLog(
        loggingInWorker(async (session) => {
          while (!session.callbacks.hasActiveLog)
            await new Promise<void>((resolve) => setTimeout(resolve, 0));
        }),
        "replacement log did not reach its handoff barrier",
      );
      check(
        active === 1 && calls === index * 2 + 1,
        "worker restart overlapped app log callbacks",
      );
      const unblock = release;
      gate = Promise.resolve();
      unblock?.();
      await waitForLog(
        second,
        "replacement log was lost after callback release",
      );
      await waitForLog(
        loggingInWorker(async (session) => {
          while (session.callbacks.hasActiveLog)
            await new Promise<void>((resolve) => setTimeout(resolve, 0));
        }),
        "replacement callback did not finish",
      );
      check(
        maxActive === 1 && active === 0,
        "app callbacks were not serial across worker failures",
      );
    }
    check(calls === 4, "worker failure lost or repeated a retained record");
  } finally {
    release?.();
    await sdk.setLogSink();
    await admin.end();
  }
}
