import type { WireEndpoint } from "../wire.js";
import { MainSession } from "./session.js";

interface Generation {
  session: MainSession;
  opening: Promise<MainSession>;
  creations: number;
  managed: boolean;
}

/** Owns the current worker generation and shares its opening handshake. */
export class WorkerSessions {
  private current?: Generation;

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
      !generation.session.isIdle
    )
      return;
    this.current = undefined;
    generation.session.terminate();
  }

  private generation(): Generation {
    if (this.current && !this.current.session.isTerminated) return this.current;
    let generation: Generation | undefined = undefined;
    const session = new MainSession(
      this.createEndpoint(),
      this.version,
      this.hash,
      () => {
        if (generation) this.retireIfIdle(generation);
      },
    );
    generation = {
      session,
      opening: session.ready().then(() => session),
      creations: 0,
      managed: false,
    };
    this.current = generation;
    void generation.opening.catch(() => {
      if (this.current === generation) this.current = undefined;
    });
    return generation;
  }

  terminate(): void {
    const generation = this.current;
    this.current = undefined;
    generation?.session.terminate();
  }
}
