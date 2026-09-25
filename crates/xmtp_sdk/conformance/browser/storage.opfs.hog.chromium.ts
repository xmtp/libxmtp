let worker: Worker | undefined;

function command(action: "hold" | "release"): Promise<void> {
  worker ??= new Worker(new URL("./storage.opfs.hog.worker.ts", import.meta.url), {
    type: "module",
  });
  const current = worker;
  return new Promise<void>((resolve, reject) => {
    current.addEventListener(
      "message",
      (event: MessageEvent<{ ok: boolean; message?: string }>) => {
        if (event.data.ok) resolve();
        else reject(new Error(event.data.message));
      },
      { once: true },
    );
    current.postMessage(action);
  });
}

export function hold(): Promise<void> {
  return command("hold");
}

export function release(): Promise<void> {
  return command("release");
}

export function stop(): void {
  worker?.terminate();
  worker = undefined;
}
