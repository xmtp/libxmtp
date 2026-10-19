import { foundationCopy } from "@/copy/foundation";
import { SessionCopy } from "@/core/sessionError.ts";

/** Startup gate strings. */
export const startupCopy = {
  storageTitle: SessionCopy.storageTitle,
  storageMessage: SessionCopy.storageMessage,
  retry: foundationCopy.retry,
} as const;
