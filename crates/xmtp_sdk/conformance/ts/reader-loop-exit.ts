type Message = { readonly id: string };
type Reader = {
  next(): Promise<Message | undefined>;
  end(): Promise<void>;
};
type Group = { messageReader(): Promise<Reader> };
type CloseReason = { readonly kind: "closed" | "failed" };
type Options = {
  signal: AbortSignal;
  onClose: (reason: CloseReason) => void;
};

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function check(value: unknown, label: string): asserts value {
  if (!value) throw new Error(label);
}

async function within<T>(value: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      value,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} timed out`)),
          10_000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

// Both public packages use this proof. The app body holds the item, so an
// abort cannot race with a later read that acknowledges it.
// verifies: PROC-052, PROC-031, PROC-041
export async function checkReaderLoopExit<G extends Group>(
  create: () => Promise<{ group: G; id: string }>,
  open: (group: G, options: Options) => AsyncIterable<Message>,
): Promise<void> {
  for (const mode of ["break", "throw", "cancel"] as const) {
    const { group, id } = await create();
    const received = latch();
    const releaseBody = latch();
    const closed = latch();
    const reasons: CloseReason[] = [];
    const controller = new AbortController();
    const appError = new Error("consumer failed");
    const stream = open(group, {
      signal: controller.signal,
      onClose: (reason) => {
        reasons.push(reason);
        closed.resolve();
      },
    });
    const consumer = (async () => {
      try {
        for await (const value of stream) {
          check(value.id === id, `${mode} received the wrong message`);
          received.resolve();
          await releaseBody.promise;
          if (mode === "throw") throw appError;
          break;
        }
      } catch (error) {
        if (mode !== "throw" || error !== appError) throw error;
      }
    })();
    try {
      // A consumer error must fail the test, rather than wait for a gate it
      // cannot reach. Normal completion still needs the gate to be signaled.
      await within(
        Promise.race([received.promise, consumer.then(() => received.promise)]),
        `${mode} handoff`,
      );
      if (mode === "cancel") controller.abort();
      else releaseBody.resolve();
      await within(closed.promise, `${mode} automatic close`);
      check(reasons[0].kind === "closed", `${mode} reported a reader failure`);
      // Reopen before the aborting app body returns. Only automatic cleanup
      // can release the original lease at this point.
      const replay = await group.messageReader();
      try {
        check(
          (await within(replay.next(), `${mode} replay`))?.id === id,
          `${mode} acknowledged the held item`,
        );
      } finally {
        await replay.end();
      }
    } finally {
      releaseBody.resolve();
      await within(consumer, `${mode} loop exit`);
    }
    check(reasons.length === 1, `${mode} notified close more than once`);
  }
}
