// SwiftUI date formats in en-US, built by hand: `Intl` gives "Oct 14, 4:03 AM"
// where SwiftUI shows "Oct 14 at 4:03 AM", and Hermes' Intl support varies.
// Apple's formatters put a narrow no-break space (U+202F) before AM and PM.

const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
] as const;

/** The space SwiftUI puts between the time and AM or PM. */
const NARROW_NO_BREAK_SPACE = "\u202f";

function month(date: Date): string {
  return MONTHS.at(date.getMonth()) ?? "";
}

/** `.dateTime.hour().minute()`: "4:03 AM" (local time, 12-hour clock). */
export function timeOfDay(date: Date): string {
  const hours = date.getHours();
  const hour = hours % 12 === 0 ? 12 : hours % 12;
  const minutes = String(date.getMinutes()).padStart(2, "0");
  return `${String(hour)}:${minutes}${NARROW_NO_BREAK_SPACE}${hours < 12 ? "AM" : "PM"}`;
}

/**
 * `.dateTime.month().day().hour().minute()`: "Oct 14 at 4:03 AM". Used by the
 * chat date separator, Send needs review and shared media.
 */
export function messageDateTime(date: Date): string {
  return `${month(date)} ${String(date.getDate())} at ${timeOfDay(date)}`;
}

/** `.formatted(date: .abbreviated, time: .omitted)`: "Oct 18, 2026". */
export function abbreviatedDate(date: Date): string {
  return `${month(date)} ${String(date.getDate())}, ${String(date.getFullYear())}`;
}
