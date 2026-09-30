import type { WireMessage } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";

export function signal() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

/** Hold a real package worker reply before the package can hand it to the app. */
export function controlPackageWorker() {
  const OriginalWorker = globalThis.Worker;
  let worker: ControlledWorker | undefined;
  const terminated = signal();
  const remember = (value: ControlledWorker) => {
    worker = value;
  };
  class ControlledWorker extends OriginalWorker {
    attachmentCreates = 0;
    private eventRead?: number;
    private holdRead = false;
    private holdCallback = false;
    private held?: MessageEvent<WireMessage>;
    private releasing = false;
    readonly arrived = signal();
    readonly callbackFinished = signal();
    private callbackId?: number;
    private readPosted = signal();

    constructor(url: string | URL, options?: WorkerOptions) {
      super(url, options);
      if (worker) throw new Error("unexpected extra package worker");
      remember(this);
      this.addEventListener("message", (event: MessageEvent<WireMessage>) => {
        if (this.releasing) return;
        const reply = event.data;
        if (
          (this.holdRead &&
            reply.t === "return" &&
            reply.id === this.eventRead &&
            reply.value != null) ||
          (this.holdCallback &&
            reply.t === "callback" &&
            reply.method === "onEvent")
        ) {
          event.stopImmediatePropagation();
          this.held = event;
          this.holdRead = false;
          this.holdCallback = false;
          if (reply.t === "callback") this.callbackId = reply.id;
          this.arrived.resolve();
        }
      });
    }

    override postMessage(
      message: WireMessage,
      transferOrOptions?: Transferable[] | StructuredSerializeOptions,
    ): void {
      if (message.t === "call" && message.key === "EventReader.next")
        this.readPosted.resolve();
      if (message.t === "call" && message.key === "Attachments.create")
        this.attachmentCreates++;
      if (
        this.holdRead &&
        message.t === "call" &&
        message.key === "EventReader.next"
      )
        this.eventRead = message.id;
      if (message.t === "callbackResult" && message.id === this.callbackId)
        this.callbackFinished.resolve();
      if (Array.isArray(transferOrOptions))
        super.postMessage(message, transferOrOptions);
      else super.postMessage(message, transferOrOptions);
    }

    watchEventRead(): Promise<void> {
      this.readPosted = signal();
      return this.readPosted.promise;
    }
    holdEvent(): void {
      this.holdRead = true;
    }
    holdListener(): void {
      this.holdCallback = true;
    }
    release(): void {
      const held = this.held;
      if (!held) throw new Error("no held worker message");
      this.held = undefined;
      this.releasing = true;
      try {
        this.dispatchEvent(new MessageEvent("message", { data: held.data }));
      } finally {
        this.releasing = false;
      }
    }
    fail(): void {
      // Kill the actual worker, then deliver the platform error notification.
      // The package manager must perform its own reservation cleanup.
      super.terminate();
      this.dispatchEvent(new Event("error"));
    }
    override terminate(): void {
      super.terminate();
      terminated.resolve();
    }
  }
  globalThis.Worker = ControlledWorker;
  return {
    get worker(): ControlledWorker {
      if (!worker) throw new Error("the package did not create a worker");
      return worker;
    },
    terminated: terminated.promise,
    restore(): void {
      globalThis.Worker = OriginalWorker;
    },
  };
}
