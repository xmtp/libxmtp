// Disappearing message durations (DetailsModels.swift `DisappearingRules` and
// Features/Disappearing/DisappearingFeatureView.swift).

/** The presets, in screen order. Tapping one saves at once. */
export const DISAPPEARING_PRESETS = [
  { label: "Off", seconds: "0" },
  { label: "1 hour", seconds: "3600" },
  { label: "8 hours", seconds: "28800" },
  { label: "1 day", seconds: "86400" },
  { label: "1 week", seconds: "604800" },
] as const;

/** `Int64.max / 1_000_000_000`: the longest duration whose nanoseconds fit in Int64. */
export const MAX_DISAPPEARING_SECONDS = 9_223_372_036n;

const INT64_MAX = 9_223_372_036_854_775_807n;
const NANOSECONDS_PER_SECOND = 1_000_000_000n;

export const DISAPPEARING_ERROR =
  "Enter a whole number of seconds from 0 to 9223372036.";

/** A duration the screen rejects before any write. */
export class DisappearingDurationError extends Error {
  constructor() {
    super(DISAPPEARING_ERROR);
    this.name = "DisappearingDurationError";
  }
}

/**
 * The retention in nanoseconds for typed seconds, or undefined for 0 (off).
 * The text must be a whole Int64 number (an optional sign, then digits) from
 * 0 to 9223372036; anything else throws `DisappearingDurationError`.
 */
export function disappearingNanoseconds(text: string): bigint | undefined {
  if (!/^[+-]?[0-9]+$/u.test(text)) throw new DisappearingDurationError();
  const seconds = BigInt(text);
  if (seconds < 0n || seconds > INT64_MAX) {
    throw new DisappearingDurationError();
  }
  if (seconds * NANOSECONDS_PER_SECOND > INT64_MAX) {
    throw new DisappearingDurationError();
  }
  return seconds === 0n ? undefined : seconds * NANOSECONDS_PER_SECOND;
}

/** The seconds text for a loaded retention (whole seconds, rounded down). */
export function secondsText(retentionNs: bigint): string {
  return (retentionNs / NANOSECONDS_PER_SECOND).toString();
}

/** True when `seconds` is one of the presets; otherwise Custom is selected. */
export function isPresetDuration(seconds: string): boolean {
  return DISAPPEARING_PRESETS.some((preset) => preset.seconds === seconds);
}
