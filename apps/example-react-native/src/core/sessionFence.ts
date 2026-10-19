// Session generations (ExampleCore/Session/SessionFence.swift). Every async
// result captures a key when it starts and is dropped unless the fence still
// accepts that key, so a late result from an ended or replaced session cannot
// publish a client, write state or navigate.

/** One opened session: its profile and the generation it was bound in. */
export interface SessionKey {
  readonly profileId: string;
  readonly generation: number;
}

export class SessionFence {
  #generation = 0;
  #profileId: string | undefined;
  #explicitlyReserved = false;

  /**
   * An explicit user action (connect, sign out, delete, continue deletion,
   * retry): a new generation with no profile bound. It always wins over an
   * automatic restore, for the rest of the process.
   */
  reserve(): number {
    this.#explicitlyReserved = true;
    this.#generation += 1;
    this.#profileId = undefined;
    return this.#generation;
  }

  /**
   * An automatic restore: a new generation, unless an explicit action ever
   * happened or a profile is bound.
   */
  reserveRestore(): number | undefined {
    if (this.#explicitlyReserved || this.#profileId !== undefined) {
      return undefined;
    }
    this.#generation += 1;
    return this.#generation;
  }

  currentGeneration(): number {
    return this.#generation;
  }

  /** Binds `profileId` to the reserved generation, or returns undefined if it is stale. */
  bind(profileId: string, reserved: number): SessionKey | undefined {
    if (!this.#isReserved(reserved)) return undefined;
    this.#profileId = profileId;
    return { profileId, generation: this.#generation };
  }

  /** True while `key` is the bound session. */
  accepts(key: SessionKey): boolean {
    return (
      this.#generation === key.generation && this.#profileId === key.profileId
    );
  }

  /** True when `reserved` is current and no profile is bound to it yet. */
  #isReserved(reserved: number): boolean {
    return this.#generation === reserved && this.#profileId === undefined;
  }
}
