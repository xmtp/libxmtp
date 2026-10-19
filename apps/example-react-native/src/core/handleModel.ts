// The Choose handle and Change handle state machine, with the SDK and
// preferences behind a small port so it runs under `node --test`.

import {
  type HandleEditorState,
  type HandleError,
  type HandleMode,
  type HandleNotice,
  type HandleStatus,
  availabilityStatus,
  canClaim,
  effectiveHandleMinimum,
  handleName,
  handleProblem,
  isEditable,
  normalizeHandle,
  selectSuggestion,
  showMoreSuggestions,
  statusPick,
  tooSoonNotice,
} from "./handles.ts";
import { Store } from "./store.ts";

/** A handle failure from the SDK, mapped by the session layer. */
export class HandleFailure extends Error {
  readonly kind: HandleError;

  constructor(kind: HandleError) {
    super(`Handle request failed: ${kind}`);
    this.name = "HandleFailure";
    this.kind = kind;
  }
}

function failureKind(error: unknown): HandleError {
  return error instanceof HandleFailure ? error.kind : "failed";
}

/** The account operations the handle screens use (`HandleAccount`). */
export interface HandleAccount {
  /** The server's minimum name length (any value; the model clamps it). */
  readonly minimum: number;
  /** Local claim times (`feature-preferences.claimTimes`). */
  claimTimes(): ReadonlyMap<string, Date>;
  /** The own handles (`inboxState(false)`). */
  handles(): Promise<{
    readonly current?: string;
    readonly others: readonly string[];
  }>;
  /** Free handles for a name: the input when free, else coded suggestions. */
  availableHandles(name: string): Promise<readonly string[]>;
  claimHandle(handle: string): Promise<void>;
  /** Saves the claim time on this device. May throw. */
  recordClaim(handle: string, at: Date): void;
}

export interface HandleModelOptions {
  readonly account: HandleAccount;
  readonly mode: HandleMode;
  /** True while the session that opened the screen is current. */
  readonly isSessionCurrent: () => boolean;
  /** Leaves the screen: Chats (choose) or back to Settings (change). */
  readonly finish: () => void;
  /** Saves "Skip for now" (choose mode). May throw. */
  readonly persistSkip?: () => void;
}

export interface HandleState extends HandleEditorState {
  readonly mode: HandleMode;
  readonly current?: string;
  /** The local claim time of the current handle ("Yours since"). */
  readonly currentSince?: Date;
  readonly others: readonly string[];
  readonly minimum: number;
  /** The handle a confirmation alert asks about (change mode). */
  readonly confirm?: string;
}

/** The Retry button shows for these notices. */
export function canRetryNotice(notice: HandleNotice | undefined): boolean {
  switch (notice?.kind) {
    case "recordFailed":
    case "claimFailed":
    case "checkFailed":
    case "skipFailed":
      return true;
    default:
      return false;
  }
}

/** Every async result is admitted only for its own attempt on an open screen. */
export class HandleModel extends Store<HandleState> {
  readonly #account: HandleAccount;
  readonly #isSessionCurrent: () => boolean;
  readonly #finish: () => void;
  readonly #persistSkip: (() => void) | undefined;
  #attempt = 0;
  #loadAttempt = 0;
  #closed = false;
  #pendingRecord: { readonly handle: string; readonly at: Date } | undefined;

  constructor(options: HandleModelOptions) {
    super({
      mode: options.mode,
      loading: true,
      others: [],
      minimum: effectiveHandleMinimum(options.account.minimum),
      input: "",
      problem: undefined,
      status: { kind: "editing" },
      claiming: undefined,
      notice: undefined,
    });
    this.#account = options.account;
    this.#isSessionCurrent = options.isSessionCurrent;
    this.#finish = options.finish;
    this.#persistSkip = options.persistSkip;
  }

  #admitted(): boolean {
    return !this.#closed && this.#isSessionCurrent();
  }

  #apply(number: number, change: (state: HandleState) => HandleState): boolean {
    if (number !== this.#attempt || !this.#admitted()) return false;
    this.setState(change);
    return true;
  }

  /** Cancels the running check or claim result. */
  #cancel(): number {
    this.#attempt += 1;
    return this.#attempt;
  }

  /**
   * Reads the own handles. In choose mode an existing handle finishes. It
   * reopens a disposed model, because React runs a mount effect again after
   * its cleanup (Fast Refresh, Strict Mode).
   */
  async load(): Promise<void> {
    this.#closed = false;
    this.#loadAttempt += 1;
    const number = this.#loadAttempt;
    const admitted = () => number === this.#loadAttempt && this.#admitted();
    try {
      const own = await this.#account.handles();
      if (!admitted()) return;
      const since =
        own.current === undefined
          ? undefined
          : this.#account.claimTimes().get(own.current);
      this.setState((state) => {
        const { current: _current, currentSince: _since, ...rest } = state;
        return {
          ...rest,
          loading: false,
          others: own.others,
          ...(own.current === undefined ? {} : { current: own.current }),
          ...(since === undefined ? {} : { currentSince: since }),
        };
      });
      if (
        own.current !== undefined &&
        this.#pendingRecord === undefined &&
        this.getState().mode === "choose"
      ) {
        this.#finish();
      }
    } catch {
      if (admitted()) this.setState((state) => ({ ...state, loading: false }));
    }
  }

  /** A keystroke: ASCII is lowercased; an unchanged value keeps the status. */
  changeInput(value: string): void {
    const state = this.getState();
    if (this.#closed || !isEditable(state)) return;
    const input = normalizeHandle(value);
    if (input === state.input) return;
    this.#cancel();
    const { confirm: _confirm, ...rest } = state;
    this.setState({
      ...rest,
      input,
      problem: handleProblem(input, state.minimum),
      status: { kind: "editing" },
      notice: undefined,
    });
  }

  /** "Continue": checks availability (or confirms switching back to an own handle). */
  check(): void {
    const state = this.getState();
    if (
      this.#closed ||
      !isEditable(state) ||
      state.input === "" ||
      state.problem !== undefined
    ) {
      return;
    }
    if (state.input === state.current) {
      this.setState({ ...state, problem: "current" });
      return;
    }
    if (state.mode === "change" && state.others.includes(state.input)) {
      this.setState({ ...state, confirm: state.input });
      return;
    }
    const input = state.input;
    this.setState({
      ...state,
      status: { kind: "checking" },
      notice: undefined,
    });
    const number = this.#cancel();
    void this.#answer(number, input);
  }

  /** "Show more": three more suggestions. */
  showMore(): void {
    this.setState((state) => ({
      ...state,
      status: showMoreSuggestions(state.status),
    }));
  }

  /** Picks a visible suggestion. */
  select(handle: string): void {
    const state = this.getState();
    if (!isEditable(state)) return;
    this.setState({ ...state, status: selectSuggestion(state.status, handle) });
  }

  /** The primary button: claims now (choose) or asks to confirm (change). */
  claim(): void {
    const state = this.getState();
    const handle = statusPick(state.status);
    if (this.#closed || !canClaim(state) || handle === undefined) return;
    if (state.mode === "change") {
      this.setState({ ...state, confirm: handle });
    } else {
      this.#claim(handle);
    }
  }

  /** "Use" on another own handle (change mode): asks to confirm. */
  switchBack(handle: string): void {
    const state = this.getState();
    if (
      isEditable(state) &&
      state.mode === "change" &&
      state.others.includes(handle)
    ) {
      this.setState({ ...state, confirm: handle });
    }
  }

  /** The confirmation alert's Claim or Change. */
  confirmClaim(): void {
    const handle = this.getState().confirm;
    if (handle !== undefined) this.#claim(handle);
  }

  /** The confirmation alert's Cancel. */
  cancelClaim(): void {
    const { confirm: _confirm, ...rest } = this.getState();
    this.setState(rest);
  }

  /** "Retry" under a notice. */
  retry(): void {
    const notice = this.getState().notice;
    switch (notice?.kind) {
      case "recordFailed":
        this.#persistRecord();
        return;
      case "skipFailed":
        this.skip();
        return;
      case "claimFailed":
        this.#claim(notice.handle);
        return;
      case "checkFailed":
        this.check();
        return;
      default:
        return;
    }
  }

  /** "Skip for now" (choose mode): saves the choice, then finishes. */
  skip(): void {
    const state = this.getState();
    if (
      state.mode !== "choose" ||
      state.claiming !== undefined ||
      this.#pendingRecord !== undefined ||
      !this.#admitted()
    ) {
      return;
    }
    const number = this.#cancel();
    try {
      this.#persistSkip?.();
      if (this.#apply(number, (current) => ({ ...current, notice: undefined })))
        this.#finish();
    } catch {
      this.#apply(number, (current) => ({
        ...current,
        notice: { kind: "skipFailed" },
      }));
    }
  }

  /** The screen closed: results that arrive later are dropped. A claim still records its time. */
  dispose(): void {
    this.#closed = true;
    this.#cancel();
  }

  async #answer(
    number: number,
    input: string,
    except?: string,
    notice?: HandleNotice,
  ): Promise<void> {
    try {
      const free = (await this.#account.availableHandles(input)).filter(
        (handle) => handle !== except,
      );
      let status: HandleStatus;
      if (free.includes(input)) {
        status = { kind: "available", handle: input };
      } else {
        const name = handleName(input);
        const alternatives =
          name === input ? free : await this.#account.availableHandles(name);
        status = availabilityStatus(input, free, alternatives, except);
      }
      this.#apply(number, (state) => ({
        ...state,
        claiming: undefined,
        status,
        notice,
      }));
    } catch (error) {
      const kind = failureKind(error);
      this.#apply(number, (state) => ({
        ...state,
        claiming: undefined,
        status: { kind: "editing" },
        ...(kind === "invalidArgument"
          ? { problem: "invalidCharacters" as const }
          : { notice: { kind: "checkFailed" as const } }),
      }));
    }
  }

  #persistRecord(): void {
    const pending = this.#pendingRecord;
    if (this.#closed || !this.#admitted() || pending === undefined) return;
    try {
      this.#account.recordClaim(pending.handle, pending.at);
      this.#pendingRecord = undefined;
      if (
        this.#apply(this.#attempt, (state) => ({ ...state, notice: undefined }))
      ) {
        this.#finish();
      }
    } catch {
      this.#apply(this.#attempt, (state) => ({
        ...state,
        notice: { kind: "recordFailed", handle: pending.handle },
      }));
    }
  }

  #claim(handle: string): void {
    const previous = this.getState();
    if (!isEditable(previous)) return;
    const { confirm: _confirm, ...rest } = previous;
    this.setState({ ...rest, claiming: handle, notice: undefined });
    const number = this.#cancel();
    void (async () => {
      try {
        await this.#account.claimHandle(handle);
      } catch (error) {
        await this.#claimFailed(number, handle, previous, failureKind(error));
        return;
      }
      if (!this.#isSessionCurrent()) return;
      const now = new Date();
      if (this.#closed || number !== this.#attempt) {
        // The claim happened: keep its time even though the screen moved on.
        try {
          this.#account.recordClaim(handle, now);
        } catch {
          // Nothing shows it any more.
        }
        return;
      }
      this.#pendingRecord = { handle, at: now };
      const others = [
        ...(previous.current === undefined ? [] : [previous.current]),
        ...previous.others,
      ].filter((other) => other !== handle);
      if (
        this.#apply(number, (state) => ({
          ...state,
          claiming: undefined,
          current: handle,
          currentSince: now,
          others,
          input: "",
          problem: undefined,
          status: { kind: "editing" },
          notice: undefined,
        }))
      ) {
        this.#persistRecord();
      }
    })();
  }

  async #claimFailed(
    number: number,
    handle: string,
    previous: HandleState,
    kind: HandleError,
  ): Promise<void> {
    if (kind === "taken") {
      await this.#answer(
        number,
        previous.input === "" ? handle : previous.input,
        handle,
        { kind: "taken", handle },
      );
      return;
    }
    this.#apply(number, (state) => {
      const cleared = {
        ...state,
        claiming: undefined,
        input: "",
        problem: undefined,
        status: { kind: "editing" } as const,
      };
      switch (kind) {
        case "notAllowed":
          return {
            ...state,
            claiming: undefined,
            status: { kind: "editing" },
            notice: previous.others.includes(handle)
              ? {
                  kind: "switchBackBlocked",
                  handle,
                  shorter: handleName(handle).length < state.minimum,
                }
              : { kind: "notAllowed", name: handleName(handle) },
          };
        case "tooSoon":
          return {
            ...cleared,
            notice: tooSoonNotice(
              this.#account.claimTimes().values(),
              new Date(),
            ),
          };
        case "limitReached":
          return { ...cleared, notice: { kind: "limitReached" } };
        default:
          return {
            ...state,
            claiming: undefined,
            notice: { kind: "claimFailed", handle },
          };
      }
    });
  }
}
