type StopStream = () => void;
type StartStream = () => Promise<StopStream>;

/** Owns streams across async setup, refresh, and view cleanup. */
export const createStreamSession = (startStreams: StartStream[]) => {
  let generation = 0;
  let stops: StopStream[] = [];

  const stop = () => {
    generation += 1;
    const currentStops = stops;
    stops = [];
    for (const stopStream of currentStops) {
      stopStream();
    }
  };

  const start = async (sync: () => Promise<unknown>) => {
    stop();
    const currentGeneration = generation;
    try {
      await sync();
      for (const startStream of startStreams) {
        if (currentGeneration !== generation) return;
        const stopStream = await startStream();
        if (currentGeneration !== generation) {
          stopStream();
          return;
        }
        stops.push(stopStream);
      }
    } catch (error) {
      // An older setup must not close streams from a newer refresh.
      if (currentGeneration === generation) stop();
      throw error;
    }
  };

  return { start, stop };
};
