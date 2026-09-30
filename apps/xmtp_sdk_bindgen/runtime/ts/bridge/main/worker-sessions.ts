import { bridgeError, type WireEndpoint } from "../wire.js";
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
  private retiring?: MainSession;

  constructor(
    private readonly createEndpoint: () => WireEndpoint,
    private readonly version: number,
    private readonly hash: string,
  ) {}

  get(): Promise<MainSession> {
    return this.generation().opening;
  }

  /** Reserve before the handshake or any caller work can await. */
  async create<T>(create: (session: MainSession) => Promise<T>): Promise<T> {
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
        );
        generation.session = session;
        void session.ready().then(() => resolveOpening(session), rejectOpening);
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
