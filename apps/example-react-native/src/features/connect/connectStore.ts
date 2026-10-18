import { parseBackendUrl } from "@/core/backendUrl.ts";
import { errorMessage, SessionError } from "@/core/sessionError.ts";
import { Store } from "@/core/store.ts";
import { connectCopy } from "@/features/connect/copy";
import { probeBackend, sessionController } from "@/session";

export interface ConnectState {
  readonly address: string;
  readonly name: string;
  readonly joining: boolean;
  /** The last Join or recovery failure, shown in red. */
  readonly error?: string;
}

/** Join is enabled for a valid address while nothing runs (`ConnectState.canJoin`). */
export function canJoin(state: ConnectState): boolean {
  return !state.joining && parseBackendUrl(state.address) !== undefined;
}

/**
 * The Connect route store (a basic `ConnectStore.swift`): Join probes the
 * server, then opens the session. The session router leaves Connect when the
 * session opens. A server that needs an access token is reported, not joined.
 */
export class ConnectStore extends Store<ConnectState> {
  #disposed = false;

  constructor(address: string) {
    super({ address, name: "", joining: false });
  }

  setAddress(address: string): void {
    this.setState((state) => ({ ...state, address, error: undefined }));
  }

  setName(name: string): void {
    this.setState((state) => ({ ...state, name }));
  }

  async join(): Promise<void> {
    const state = this.getState();
    const backend = parseBackendUrl(state.address);
    if (!canJoin(state) || backend === undefined) return;
    this.setState({ ...state, joining: true, error: undefined });
    await this.#run(async () => {
      let requiresToken: boolean;
      try {
        ({ requiresToken } = await probeBackend(backend));
      } catch {
        throw SessionError.message(connectCopy.unreachable);
      }
      if (requiresToken) throw SessionError.message(connectCopy.enterToken);
      const name = state.name.trim();
      await sessionController.connect({
        url: state.address,
        token: "",
        ...(name === "" ? {} : { name }),
      });
    });
  }

  /** Restore failed: "Retry". */
  retryRestore(): Promise<void> {
    return this.#run(() => sessionController.retryRestore());
  }

  /** Restore failed: "Delete local data" after the confirmation. */
  deleteLocalData(): Promise<void> {
    return this.#run(() => sessionController.deleteLocalData());
  }

  /** Reset incomplete: "Continue local data removal". */
  continueReset(): Promise<void> {
    return this.#run(() => sessionController.continueReset());
  }

  /** The route unmounted. Work that is running finishes; its result is dropped. */
  dispose(): void {
    this.#disposed = true;
  }

  /**
   * Runs an action and shows its failure. A successful Join keeps "Joining
   * server…" until the router replaces Connect with the session's first screen.
   */
  async #run(work: () => Promise<void>): Promise<void> {
    try {
      await work();
      if (this.#disposed) return;
      this.setState((current) => ({ ...current, error: undefined }));
    } catch (error) {
      if (this.#disposed) return;
      this.setState((current) => ({
        ...current,
        joining: false,
        error: errorMessage(error),
      }));
    }
  }
}
