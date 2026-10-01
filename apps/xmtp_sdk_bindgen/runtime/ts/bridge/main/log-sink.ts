import type { MainSession } from "./session.js";

/** Order accepted settings and restore them before a new worker can serve callers. */
export function logSinkSetter<Sink>(
  inWorker: (update: (session: MainSession) => Promise<void>) => Promise<void>,
  setWorkerSink: (session: MainSession, sink?: Sink) => Promise<void>,
) {
  let updates = Promise.resolve();
  let committedSink: Sink | undefined;
  let hasSinkUpdate = false;
  let firstConfiguration: ((session: MainSession) => Promise<void>) | undefined;
  let lastConfiguration: ((session: MainSession) => Promise<void>) | undefined;

  function ordered<T>(apply: () => Promise<T>): Promise<T> {
    const update = updates.then(apply);
    // A failed update does not prevent the next caller from changing settings.
    updates = update.then(
      () => {},
      () => {},
    );
    return update;
  }

  const set = (sink?: Sink) =>
    ordered(async () => {
      await inWorker((session) =>
        session.callbacks.updateLogSink(() => setWorkerSink(session, sink)),
      );
      committedSink = sink;
      hasSinkUpdate = true;
    });

  return Object.assign(set, {
    configure(
      configuration: (session: MainSession) => Promise<void>,
    ): Promise<void> {
      return ordered(async () => {
        await inWorker(configuration);
        // The first call chooses the layers. Later calls update the level.
        firstConfiguration ??= configuration;
        lastConfiguration = configuration;
      });
    },
    async initialize(session: MainSession): Promise<void> {
      // Do not await the update chain here: its caller can be waiting for this session.
      await firstConfiguration?.(session);
      if (lastConfiguration !== firstConfiguration)
        await lastConfiguration?.(session);
      if (hasSinkUpdate)
        await session.callbacks.updateLogSink(() =>
          setWorkerSink(session, committedSink),
        );
    },
  });
}
