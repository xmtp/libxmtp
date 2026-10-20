import { commonCopy } from "@/copy/common";

/**
 * Choose handle and Change handle strings. Grammar and notice copy lives in
 * `@/core/handles.ts`.
 */
export const handleCopy = {
  heroTitle: "Choose your handle.",
  heroSubtitle:
    "Your handle is public. People on this server use it to find you and send you messages.",
  yourHandle: "Your handle",
  placeholder: "nick",
  showMore: "Show more",
  lookAgain: "Look again",
  retry: commonCopy.retry,
  skip: "Skip for now",
  chooseFootnote: "You can change your handle once every 7 days.",
  changeFootnote:
    "You can change your handle once every 7 days, up to 64 times in total.",
  currentHandle: "Current handle",
  newHandle: "New handle",
  otherHandles: "Your other handles",
  otherHandlesDetail:
    "They are still yours, and people can still find you by them. Switching back counts as a change.",
  use: "Use",
  claimTitle: "Claim this handle?",
  changeTitle: "Change your handle?",
  claim: "Claim",
  change: "Change",
  cancel: commonCopy.cancel,
  yoursSince: (date: string) => `Yours since ${date}`,
  taken: (input: string) => `${input} is taken. These handles are free:`,
  available: (handle: string) => `${handle} is available.`,
  noneFree: (input: string) =>
    `${input} and the codes we checked are all taken. Look again, or try a different name.`,
  confirmMessage: (handle: string) =>
    `People will see ${handle} as your handle. You cannot change your handle again for 7 days.`,
} as const;
