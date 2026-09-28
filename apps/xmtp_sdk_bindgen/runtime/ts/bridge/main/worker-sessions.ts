import type { WireEndpoint } from "../wire.js";
import { MainSession } from "./session.js";

interface Generation {
  session: MainSession;
  opening: Promise<MainSession>;
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
    if (this.current && !this.current.session.isTerminated)
      return this.current.opening;

    const session = new MainSession(
      this.createEndpoint(),
      this.version,
      this.hash,
    );
    const generation: Generation = {
      session,
      opening: session.ready().then(() => session),
    };
    this.current = generation;
    void generation.opening.catch(() => {
      if (this.current === generation) this.current = undefined;
    });
    return generation.opening;
  }

  terminate(): void {
    const generation = this.current;
    this.current = undefined;
    generation?.session.terminate();
  }
}
