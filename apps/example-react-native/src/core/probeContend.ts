// The session probe's `contend` action (Debug only). A second default message
// reader holds the consumer lease for `CONTEND_HOLD_MS`, so the hub's reader
// fails with `ConsumerOwned` and shows the retry banner until the rival ends.

/** How long the rival reader holds the consumer lease. */
export const CONTEND_HOLD_MS = 15_000;

/** A second default message reader (`streamAllMessages()`). */
export interface RivalReader {
  /** Rejects when the reader cannot open, for example `ConsumerOwned`. */
  ready(): Promise<void>;
  end(): Promise<void>;
}

export interface ContendPorts {
  /** The stream hub's `setForeground`: false closes its reader and frees the lease. */
  readonly setForeground: (active: boolean) => Promise<void>;
  readonly openRival: () => RivalReader;
  readonly sleep: (ms: number) => Promise<void>;
}

/**
 * Pauses the hub's reader so a rival can take the lease, restarts the hub's
 * reader, holds the lease, then ends the rival. The hub restarts even when the
 * rival cannot open (a second contend while the first holds the lease), so it
 * never stays paused in the foreground.
 */
export async function contend(ports: ContendPorts): Promise<void> {
  await ports.setForeground(false);
  let rival: RivalReader;
  try {
    rival = ports.openRival();
    await rival.ready();
  } finally {
    await ports.setForeground(true);
  }
  await ports.sleep(CONTEND_HOLD_MS);
  await rival.end();
}
