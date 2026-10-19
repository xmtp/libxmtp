// The Settings store: one device action at a time, and the copy for a failed
// one. The Settings screen keeps the confirming alert's request (an
// `AlertDialog`) in its own state; this model owns only `busy` and `error`.

import { SessionCopy, errorMessage } from "./sessionError.ts";
import type { Appearance } from "./sessionIndex.ts";
import { Store } from "./store.ts";

export interface SettingsState {
  /** A sign out or deletion is running. */
  readonly busy: boolean;
  /** The red text at the end of Settings. */
  readonly error?: string;
}

export interface SettingsPorts {
  signOut(): Promise<void>;
  deleteLocalData(): Promise<void>;
  continueReset(): Promise<void>;
  setAppearance(value: Appearance): Promise<void>;
}

export class SettingsModel extends Store<SettingsState> {
  readonly #ports: SettingsPorts;
  #disposed = false;

  constructor(ports: SettingsPorts) {
    super({ busy: false });
    this.#ports = ports;
  }

  /** "Sign out" in the "Sign out?" alert. */
  signOut(): Promise<void> {
    return this.#device(() => this.#ports.signOut(), SessionCopy.signOutFailed);
  }

  /** "Delete local data" in the "Delete local data?" alert. */
  deleteLocalData(): Promise<void> {
    return this.#device(
      () => this.#ports.deleteLocalData(),
      SessionCopy.resetIncomplete,
    );
  }

  /** "Continue deletion" after an incomplete reset. */
  async continueReset(): Promise<void> {
    try {
      await this.#ports.continueReset();
    } catch {
      this.#apply((state) => ({
        ...state,
        error: SessionCopy.resetIncomplete,
      }));
    }
  }

  /** The Appearance picker. A failed save shows its copy. */
  async setAppearance(value: Appearance): Promise<void> {
    try {
      await this.#ports.setAppearance(value);
    } catch (error) {
      this.#apply((state) => ({ ...state, error: errorMessage(error) }));
    }
  }

  /** The screen appeared (again, after a React effect cleanup). */
  appear(): void {
    this.#disposed = false;
  }

  dispose(): void {
    this.#disposed = true;
  }

  async #device(work: () => Promise<void>, failure: string): Promise<void> {
    if (this.getState().busy) return;
    this.setState((state) => ({ ...state, busy: true }));
    let error: string | undefined;
    try {
      await work();
    } catch {
      error = failure;
    }
    // A closed screen drops the error, but `busy` always clears: the same
    // model can appear again and must take the next action.
    this.setState((state) =>
      error === undefined || this.#disposed
        ? { ...state, busy: false }
        : { ...state, busy: false, error },
    );
  }

  #apply(change: (state: SettingsState) => SettingsState): void {
    if (!this.#disposed) this.setState(change);
  }
}
