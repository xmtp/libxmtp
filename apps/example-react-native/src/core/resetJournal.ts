// The resumable "Delete local data" journal, saved in the session index.
// Each phase is saved before the next step, so an interrupted reset resumes
// where it stopped.

export type ResetPhase = "STOPPING" | "DATABASE_REMOVED" | "FILES_REMOVED";

export interface ResetJournal {
  readonly profileId: string;
  readonly phase: ResetPhase;
}

export function isResetPhase(value: unknown): value is ResetPhase {
  return (
    value === "STOPPING" ||
    value === "DATABASE_REMOVED" ||
    value === "FILES_REMOVED"
  );
}

/** The steps a reset runs. `save` persists the journal (undefined clears it). */
export interface ResetSteps {
  removeDatabase(profileId: string): Promise<void>;
  removeProfileFiles(profileId: string): Promise<void>;
  removeSecrets(profileId: string): Promise<void>;
  /** Saves the advanced journal before the next step runs. */
  saveJournal(journal: ResetJournal): Promise<void>;
  /** Removes the profile from the index and clears the journal, in one save. */
  finish(profileId: string): Promise<void>;
}

/**
 * Runs the remaining reset steps from `journal.phase`: STOPPING removes the
 * database (then DATABASE_REMOVED is saved), DATABASE_REMOVED removes the
 * profile files (then FILES_REMOVED is saved), and FILES_REMOVED removes the
 * secrets and the profile entry. The caller ends the client first.
 */
export async function continueResetJournal(
  journal: ResetJournal,
  steps: ResetSteps,
): Promise<void> {
  let current = journal;
  if (current.phase === "STOPPING") {
    await steps.removeDatabase(current.profileId);
    current = { ...current, phase: "DATABASE_REMOVED" };
    await steps.saveJournal(current);
  }
  if (current.phase === "DATABASE_REMOVED") {
    await steps.removeProfileFiles(current.profileId);
    current = { ...current, phase: "FILES_REMOVED" };
    await steps.saveJournal(current);
  }
  await steps.removeSecrets(current.profileId);
  await steps.finish(current.profileId);
}
