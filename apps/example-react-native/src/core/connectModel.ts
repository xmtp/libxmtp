// The Connect screen's state machine (ExampleCore/Connect/ConnectStore.swift):
// a quiet 300 ms probe after every address change, Join that probes first when
// the server is not known to be ready, and admission of every async result.

import { type BackendUrl, parseBackendUrl } from "./backendUrl.ts";
import { CancelledError, SessionCopy, errorMessage } from "./sessionError.ts";
import { Store } from "./store.ts";

/** The probe debounce after an address change. */
export const PROBE_DELAY_MS = 300;

export interface ConnectState {
  /** The address field, raw. */
  readonly address: string;
  /** The Access token field, raw. */
  readonly token: string;
  /** The optional server name field. */
  readonly serverName: string;
  /** A probe is in flight. */
  readonly checking: boolean;
  /** The last probe for this address succeeded. */
  readonly ready: boolean;
  /** A join is in flight. */
  readonly joining: boolean;
  /** The last probe said the server requires an access token. */
  readonly requiresToken: boolean;
  /** The red status text. Probe failures stay quiet until Join. */
  readonly error?: string;
}

/**
 * Join is enabled when nothing runs, the address parses, and a token is typed
 * when the server needs one (`ConnectState.canJoin`). It does not require a
 * successful probe: Join probes again first.
 */
export function canJoin(state: ConnectState): boolean {
  return (
    !state.checking &&
    !state.joining &&
    state.address.trim() !== "" &&
    (!state.requiresToken || state.token.trim() !== "") &&
    parseBackendUrl(state.address) !== undefined
  );
}

export interface ConnectJoinRequest {
  /** The normalized URL. */
  readonly url: string;
  readonly token: string;
  readonly name?: string;
  /** Aborted when the address changes or the screen closes. */
  readonly signal: AbortSignal;
  /** Called with the session generation the join reserved. */
  readonly onReserved: (generation: number) => void;
}

export interface ConnectPorts {
  /** Reads the server configuration. Any failure means unreachable. */
  probe(url: BackendUrl): Promise<{ readonly requiresToken: boolean }>;
  /** Opens the session (`sessionController.connect`). */
  connect(request: ConnectJoinRequest): Promise<void>;
  /** The session fence's current generation. */
  generation(): number;
  /** Waits `ms`, or rejects with a `CancelledError` when `signal` aborts. */
  readonly sleep?: (ms: number, signal: AbortSignal) => Promise<void>;
}

function abortableSleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(new CancelledError());
      return;
    }
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    function onAbort() {
      clearTimeout(timer);
      reject(new CancelledError());
    }
    signal.addEventListener("abort", onAbort, { once: true });
  });
}

/**
 * The Connect route model. Every result is admitted only while the screen is
 * open, the attempt is the latest, the session generation is unchanged and
 * the address still normalizes to the URL the work started with.
 */
export class ConnectModel extends Store<ConnectState> {
  readonly #ports: ConnectPorts;
  readonly #sleep: (ms: number, signal: AbortSignal) => Promise<void>;
  #task: AbortController | undefined;
  #attempt = 0;
  #hasAttemptedJoin = false;
  #disposed = false;

  constructor(address: string, ports: ConnectPorts) {
    super({
      address,
      token: "",
      serverName: "",
      checking: false,
      ready: false,
      joining: false,
      requiresToken: false,
    });
    this.#ports = ports;
    this.#sleep = ports.sleep ?? abortableSleep;
  }

  /**
   * The screen appeared: clears the token and probes quietly. It reopens a
   * disposed model, because React runs a mount effect again after its cleanup
   * (Fast Refresh, Strict Mode).
   */
  appear(): void {
    this.#disposed = false;
    this.#hasAttemptedJoin = false;
    this.setState((state) => ({ ...state, joining: false, token: "" }));
    this.#scheduleProbe(false);
  }

  /** An address edit. The same text is a no-op; otherwise the token clears. */
  setAddress(address: string): void {
    if (this.#disposed || address === this.getState().address) return;
    this.#hasAttemptedJoin = false;
    this.setState((state) => ({
      ...state,
      address,
      token: "",
      requiresToken: false,
    }));
    this.#scheduleProbe(false);
  }

  setToken(token: string): void {
    this.setState((state) => ({ ...state, token }));
  }

  setServerName(serverName: string): void {
    this.setState((state) => ({ ...state, serverName }));
  }

  /** Status "Retry": probes again. */
  retryProbe(): void {
    if (this.#disposed) return;
    this.#scheduleProbe(false);
  }

  /**
   * "Join a server". When the server is not known to be ready it probes
   * first and joins after a successful probe.
   */
  join(): void {
    const state = this.getState();
    const url = parseBackendUrl(state.address);
    if (this.#disposed || !canJoin(state) || url === undefined) return;
    this.#hasAttemptedJoin = true;
    if (!state.ready) {
      this.#scheduleProbe(true);
      return;
    }
    const { token, serverName: name } = state;
    this.setState({ ...state, joining: true, error: undefined });
    const task = this.#restart();
    const expected = this.#attempt;
    let generation = this.#ports.generation();
    void (async () => {
      try {
        await this.#ports.connect({
          url: url.value,
          token,
          ...(name === "" ? {} : { name }),
          signal: task.signal,
          onReserved: (reserved) => {
            generation = reserved;
          },
        });
        // The bearer token never stays in UI state after a join.
        if (expected === this.#attempt && !this.#disposed) {
          this.setState((current) => ({ ...current, token: "" }));
        }
        this.#apply(expected, generation, url, (current) => ({
          ...current,
          joining: false,
        }));
      } catch (error) {
        this.#apply(expected, generation, url, (current) => ({
          ...current,
          error: errorMessage(error),
          joining: false,
        }));
      }
    })();
  }

  /** The screen closed: running work finishes, and its result is dropped. */
  dispose(): void {
    this.#disposed = true;
    this.#task?.abort();
  }

  #restart(): AbortController {
    this.#task?.abort();
    this.#attempt += 1;
    const task = new AbortController();
    this.#task = task;
    return task;
  }

  #scheduleProbe(joinWhenReady: boolean): void {
    const task = this.#restart();
    this.setState((state) => ({
      ...state,
      error: undefined,
      ready: false,
      joining: false,
    }));
    const url = parseBackendUrl(this.getState().address);
    if (url === undefined) {
      this.setState((state) => ({ ...state, checking: false }));
      return;
    }
    this.setState((state) => ({ ...state, checking: true }));
    const expected = this.#attempt;
    const generation = this.#ports.generation();
    void (async () => {
      try {
        await this.#sleep(PROBE_DELAY_MS, task.signal);
        const result = await this.#ports.probe(url);
        const decision = { join: false };
        this.#apply(expected, generation, url, (state) => {
          const next = {
            ...state,
            checking: false,
            requiresToken: result.requiresToken,
            ready: true,
          };
          decision.join = joinWhenReady && canJoin(next);
          return next;
        });
        if (decision.join) this.join();
      } catch {
        if (task.signal.aborted) return;
        this.#apply(expected, generation, url, (state) => ({
          ...state,
          checking: false,
          error: this.#hasAttemptedJoin ? SessionCopy.unreachable : undefined,
        }));
      }
    })();
  }

  #apply(
    expected: number,
    generation: number,
    url: BackendUrl,
    change: (state: ConnectState) => ConnectState,
  ): void {
    if (
      this.#disposed ||
      expected !== this.#attempt ||
      generation !== this.#ports.generation() ||
      parseBackendUrl(this.getState().address)?.value !== url.value
    ) {
      return;
    }
    this.setState(change);
  }
}
