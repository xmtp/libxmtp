import * as sdk from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
import { loggingInWorker } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/package-session.gen";
import { loggingContract } from "../ts/logging-contract.js";
import { waitForLog } from "../ts/logging-wait.js";

let terminated = 0;
let notifyTermination: (() => void) | undefined;
const OriginalWorker = globalThis.Worker;
globalThis.Worker = class extends OriginalWorker {
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
    const options: { level: sdk.LogLevel } = { level: "error" };
    await sdk.initLogging(options);
    options.level = "off";
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
