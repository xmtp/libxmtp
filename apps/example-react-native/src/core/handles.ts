// Handle grammar, state rules and copy (ExampleCore/Handles/HandleModels.swift,
// HandleCopy.swift and the pure parts of HandleStore.swift).

import { abbreviatedDate } from "./dates.ts";

/** The longest name before `#`. */
export const MAX_HANDLE_NAME_LENGTH = 27;
/** The minimum the app uses when the server reports one outside 1-27. */
export const DEFAULT_HANDLE_MINIMUM = 3;
/** A handle can change once every 7 days. */
export const HANDLE_CHANGE_INTERVAL_MS = 604_800_000;

export type HandleMode = "choose" | "change";

export type HandleProblem =
  | "invalidCharacters"
  | "startWithLetter"
  | "tooLong"
  | "tooShort"
  | "badCode"
  | "current";

/** Lowercases ASCII `A-Z` only; every other character stays (`"AbÇ"` → `"abÇ"`). */
export function normalizeHandle(value: string): string {
  return value.replace(/[A-Z]/gu, (letter) => letter.toLowerCase());
}

/** The name: the text before the first `#`. */
export function handleName(value: string): string {
  const index = value.indexOf("#");
  return index < 0 ? value : value.slice(0, index);
}

/**
 * The first grammar problem of a normalized handle, in iOS order: characters
 * outside `[a-z0-9_-]` in the name, a name that does not start with a letter,
 * a name over 27 or under `minimum` characters, then a code that is not `#`
 * and 4 digits. Empty input has no problem.
 */
export function handleProblem(
  value: string,
  minimum: number,
): HandleProblem | undefined {
  if (value === "") return undefined;
  const name = handleName(value);
  const code = value.slice(name.length);
  if (/[^a-z0-9_-]/u.test(name)) return "invalidCharacters";
  if (!/^[a-z]/u.test(name)) return "startWithLetter";
  if (name.length > MAX_HANDLE_NAME_LENGTH) return "tooLong";
  if (name.length < minimum) return "tooShort";
  if (code !== "" && !/^#[0-9]{4}$/u.test(code)) return "badCode";
  return undefined;
}

/** The server minimum when it is in 1-27, else 3 (`HandleStore.init`). */
export function effectiveHandleMinimum(serverMinimum: number): number {
  return Number.isInteger(serverMinimum) &&
    serverMinimum >= 1 &&
    serverMinimum <= MAX_HANDLE_NAME_LENGTH
    ? serverMinimum
    : DEFAULT_HANDLE_MINIMUM;
}

/** The hint under the handle field when there is no problem. */
export function handleHint(minimum: number): string {
  return `${String(minimum)} to 27 lowercase letters, numbers, _ or -.`;
}

/** The copy for a grammar problem. */
export function handleProblemCopy(
  problem: HandleProblem,
  input: string,
  minimum: number,
): string {
  switch (problem) {
    case "invalidCharacters":
      return "Use only lowercase letters, numbers, _ and -.";
    case "startWithLetter":
      return "Start with a letter.";
    case "tooLong":
      return "Use 27 characters or fewer before #.";
    case "tooShort":
      return `Use at least ${String(minimum)} characters on this server.`;
    case "badCode":
      return `After #, use exactly 4 digits, like ${handleName(input)}#0012.`;
    case "current":
      return "This is your current handle.";
  }
}

export type HandleNotice =
  | { readonly kind: "taken"; readonly handle: string }
  | { readonly kind: "notAllowed"; readonly name: string }
  | {
      readonly kind: "tooSoon";
      readonly changed: Date | undefined;
      readonly next: Date | undefined;
    }
  | { readonly kind: "limitReached" }
  | { readonly kind: "checkFailed" }
  | { readonly kind: "skipFailed" }
  | { readonly kind: "claimFailed"; readonly handle: string }
  | { readonly kind: "recordFailed"; readonly handle: string }
  | {
      readonly kind: "switchBackBlocked";
      readonly handle: string;
      readonly shorter: boolean;
    };

/** The copy for a notice. Dates use the abbreviated form ("Oct 18, 2026"). */
export function handleNoticeCopy(notice: HandleNotice): string {
  switch (notice.kind) {
    case "taken":
      return `Someone claimed ${notice.handle} a moment before you. Pick another handle.`;
    case "notAllowed":
      return `${notice.name} is not allowed on this server.`;
    case "tooSoon":
      if (notice.changed === undefined || notice.next === undefined) {
        return "You changed your handle less than 7 days ago. Try again later.";
      }
      return `You changed your handle on ${abbreviatedDate(notice.changed)}. You can change it again on ${abbreviatedDate(notice.next)}.`;
    case "limitReached":
      return "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.";
    case "checkFailed":
      return "Cannot check this handle. Try again.";
    case "skipFailed":
      return "Cannot save your preference. Try again.";
    case "claimFailed":
      return `Cannot claim ${notice.handle}. Try again.`;
    case "recordFailed":
      return `Your handle is ${notice.handle}, but its change date could not be saved. Retry to save it on this device.`;
    case "switchBackBlocked":
      return notice.shorter
        ? `${notice.handle} is shorter than this server now allows. It still finds you, but you cannot switch back to it.`
        : `${notice.handle} is not allowed on this server now. It still finds you, but you cannot switch back to it.`;
  }
}

/**
 * The tooSoon notice: the newest local claim time if it is under 7 days old,
 * and the day the next change is allowed. Unknown dates give the generic copy.
 */
export function tooSoonNotice(
  claimTimes: Iterable<Date>,
  now: Date,
): HandleNotice {
  let newest: Date | undefined;
  for (const time of claimTimes) {
    if (newest === undefined || time.getTime() > newest.getTime())
      newest = time;
  }
  const recent =
    newest !== undefined &&
    now.getTime() - newest.getTime() < HANDLE_CHANGE_INTERVAL_MS
      ? newest
      : undefined;
  return {
    kind: "tooSoon",
    changed: recent,
    next:
      recent === undefined
        ? undefined
        : new Date(recent.getTime() + HANDLE_CHANGE_INTERVAL_MS),
  };
}

export type HandleStatus =
  | { readonly kind: "editing" }
  | { readonly kind: "checking" }
  | { readonly kind: "available"; readonly handle: string }
  | {
      readonly kind: "suggestions";
      readonly input: string;
      readonly handles: readonly string[];
      readonly shown: number;
      readonly selected: string;
    }
  | { readonly kind: "noneFree"; readonly input: string };

/** The handle a claim would take: the available input or the selected suggestion. */
export function statusPick(status: HandleStatus): string | undefined {
  switch (status.kind) {
    case "available":
      return status.handle;
    case "suggestions":
      return status.selected;
    default:
      return undefined;
  }
}

/** The suggestions on screen (3 at first, 3 more per "Show more"). */
export function visibleSuggestions(status: HandleStatus): readonly string[] {
  return status.kind === "suggestions"
    ? status.handles.slice(0, status.shown)
    : [];
}

/** Whether "Show more" has more suggestions to show. */
export function hasMoreSuggestions(status: HandleStatus): boolean {
  return status.kind === "suggestions" && status.shown < status.handles.length;
}

/** "Show more": three more suggestions, up to all of them. */
export function showMoreSuggestions(status: HandleStatus): HandleStatus {
  return status.kind === "suggestions"
    ? { ...status, shown: Math.min(status.shown + 3, status.handles.length) }
    : status;
}

/** Selects a visible suggestion; any other handle leaves the status as is. */
export function selectSuggestion(
  status: HandleStatus,
  handle: string,
): HandleStatus {
  return status.kind === "suggestions" &&
    status.handles.slice(0, status.shown).includes(handle)
    ? { ...status, selected: handle }
    : status;
}

/**
 * The status after an availability check (`HandleStore.answer`). `free` is
 * `availableHandles(input)`; `alternatives` is `availableHandles(name)`, which
 * is the same list when the input has no code. `except` (a handle that was
 * just taken) never appears.
 */
export function availabilityStatus(
  input: string,
  free: readonly string[],
  alternatives: readonly string[],
  except?: string,
): HandleStatus {
  if (free.filter((handle) => handle !== except).includes(input)) {
    return { kind: "available", handle: input };
  }
  const handles = alternatives.filter(
    (handle) => handle !== input && handle !== except,
  );
  const first = handles.at(0);
  if (first === undefined) return { kind: "noneFree", input };
  return {
    kind: "suggestions",
    input,
    handles,
    shown: Math.min(3, handles.length),
    selected: first,
  };
}

/** The editor fields that the derived flags read. */
export interface HandleEditorState {
  readonly loading: boolean;
  readonly input: string;
  readonly problem: HandleProblem | undefined;
  readonly status: HandleStatus;
  readonly claiming: string | undefined;
  readonly notice: HandleNotice | undefined;
}

/** A tooSoon or limitReached notice blocks the editor. */
export function isBlocked(state: HandleEditorState): boolean {
  return (
    state.notice?.kind === "tooSoon" || state.notice?.kind === "limitReached"
  );
}

/** Editing is allowed unless a record failed, a claim runs, or the editor is blocked. */
export function isEditable(state: HandleEditorState): boolean {
  return (
    state.notice?.kind !== "recordFailed" &&
    state.claiming === undefined &&
    !isBlocked(state)
  );
}

/** Whether the primary button checks the input now. */
export function canCheck(state: HandleEditorState): boolean {
  const notice = state.notice;
  const blockedName =
    notice?.kind === "notAllowed" && notice.name === handleName(state.input);
  return (
    isEditable(state) &&
    !state.loading &&
    state.input !== "" &&
    state.problem === undefined &&
    state.status.kind === "editing" &&
    !blockedName
  );
}

/** Whether the primary button claims the picked handle. */
export function canClaim(state: HandleEditorState): boolean {
  return isEditable(state) && statusPick(state.status) !== undefined;
}

/** The primary button title: "Claiming x…", "Change to x", "Claim x" or "Continue". */
export function primaryButtonTitle(
  mode: HandleMode,
  state: HandleEditorState,
): string {
  if (state.claiming !== undefined) return `Claiming ${state.claiming}…`;
  const pick = statusPick(state.status);
  if (pick !== undefined) {
    return mode === "change" ? `Change to ${pick}` : `Claim ${pick}`;
  }
  return "Continue";
}

/** SDK handle failures, mapped from `XmtpError` in the session layer. */
export type HandleError =
  | "taken"
  | "notAllowed"
  | "tooSoon"
  | "limitReached"
  | "invalidArgument"
  | "failed";
