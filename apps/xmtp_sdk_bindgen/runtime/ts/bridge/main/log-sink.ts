import type { MainSession } from "./session.js";

/** Order sink updates, including selection of their worker session. */
export function logSinkSetter<Sink>(
  inWorker: (update: (session: MainSession) => Promise<void>) => Promise<void>,
  setWorkerSink: (session: MainSession, sink?: Sink) => Promise<void>,
): (sink?: Sink) => Promise<void> {
  let updates = Promise.resolve();
  return (sink) => {
    const update = updates.then(() =>
      inWorker((session) =>
        session.callbacks.updateLogSink(() => setWorkerSink(session, sink)),
      ),
    );
    // A failed update does not prevent the next caller from installing a sink.
    updates = update.catch(() => {});
    return update;
  };
}
