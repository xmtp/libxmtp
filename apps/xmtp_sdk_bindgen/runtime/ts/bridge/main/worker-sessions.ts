import { bridgeError, type WireEndpoint } from "../wire.js";
import { LogCallbackQueue } from "./callbacks.js";
import { MainSession } from "./session.js";

interface Generation {
  session?: MainSession;
  opening: Promise<MainSession>;
  rejectOpening(error: unknown): void;
  cancelled: boolean;
  creations: number;
  managed: boolean;
}

/** Owns the current worker generation and shares its opening handshake. */
export class WorkerSessions {
  private current?: Generation;
  private exclusive = false;
  private retiring?: MainSession;
  private readonly logQueue = new LogCallbackQueue();

  constructor(
    private readonly createEndpoint: () => WireEndpoint,
    private readonly version: number,
    private readonly hash: string,
    private readonly initialize?: (session: MainSession) => Promise<void>,
  ) {}

  get(): Promise<MainSession> {
    if (this.exclusive) return Promise.reject(bridgeError("storageBusy"));
    return this.generation().opening;
  }

  /** Reserve before the handshake or any caller work can await. */
  async create<T>(create: (session: MainSession) => Promise<T>): Promise<T> {
    if (this.exclusive) throw bridgeError("storageBusy");
    const generation = this.generation();
    generation.managed = true;
    generation.creations++;
    try {
      return await create(await generation.opening);
    } finally {
      generation.creations--;
      this.retireIfIdle(generation);
    }
  }

  /** Run storage migration alone in a fresh worker and await its release. */
  async runExclusive<T>(
    call: (session: MainSession) => Promise<T>,
  ): Promise<T> {
    if (
      this.exclusive ||
      (this.current &&
        !this.current.session?.isTerminated &&
        (this.current.creations !== 0 ||
          !this.current.session?.canRetireForMigration))
    )
      throw bridgeError("storageBusy");
    this.exclusive = true;
    if (this.current?.session) {
      this.retiring = this.current.session;
      this.current = undefined;
      this.retiring.terminate();
    }
    const generation = this.generation();
    generation.managed = true;
    generation.creations++;
    try {
      return await call(await generation.opening);
    } finally {
      generation.creations--;
      if (this.current === generation) this.current = undefined;
      try {
        if (generation.session) {
          this.retiring = generation.session;
          generation.session.terminate();
          await generation.session.whenTerminated();
        }
      } finally {
        this.exclusive = false;
      }
    }
  }

  private retireIfIdle(generation: Generation): void {
    if (
      this.current !== generation ||
      !generation.managed ||
      generation.creations !== 0 ||
      !generation.session?.isIdle
    )
      return;
    // Record the barrier before terminate can call back into a factory.
    this.retiring = generation.session;
    this.current = undefined;
    generation.session.terminate();
  }

  private generation(): Generation {
    if (this.current) {
      if (!this.current.session?.isTerminated) return this.current;
      this.retiring = this.current.session;
    }
    let resolveOpening!: (session: MainSession) => void;
    let rejectOpening!: (error: unknown) => void;
    const opening = new Promise<MainSession>((resolve, reject) => {
      resolveOpening = resolve;
      rejectOpening = reject;
    });
    const generation: Generation = {
      opening,
      rejectOpening,
      cancelled: false,
      creations: 0,
      managed: false,
    };
    this.current = generation;
    const start = () => {
      if (generation.cancelled) return;
      try {
        const session = new MainSession(
          this.createEndpoint(),
          this.version,
          this.hash,
          () => this.retireIfIdle(generation),
          this.logQueue,
        );
        generation.session = session;
        void session
          .ready()
          .then(async () => {
            await this.initialize?.(session);
            if (generation.cancelled || session.isTerminated)
              throw bridgeError("workerTerminated");
            resolveOpening(session);
          })
          .catch((error: unknown) => {
            session.terminate(error);
            rejectOpening(error);
          });
      } catch (error) {
        rejectOpening(error);
      }
    };
    void opening.catch(() => {
      if (this.current !== generation) return;
      if (generation.session?.isTerminated) this.retiring = generation.session;
      this.current = undefined;
    });
    if (this.retiring && !this.retiring.terminationComplete)
      void this.retiring.whenTerminated().then(start, rejectOpening);
    else start();
    return generation;
  }

  terminate(): void {
    const generation = this.current;
    this.current = undefined;
    if (!generation) return;
    generation.cancelled = true;
    generation.rejectOpening(bridgeError("workerTerminated"));
    if (generation.session) {
      this.retiring = generation.session;
      generation.session.terminate();
    }
  }
}
