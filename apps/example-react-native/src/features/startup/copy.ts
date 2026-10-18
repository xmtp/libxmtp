import { foundationCopy } from "@/copy/foundation";
import { SessionCopy } from "@/core/sessionError.ts";

/** Startup gate strings (`ExampleApp.swift`, `Foundation.xcstrings`). */
export const startupCopy = {
  storageTitle: SessionCopy.storageTitle,
  storageMessage: SessionCopy.storageMessage,
  retry: foundationCopy.retry,
} as const;
