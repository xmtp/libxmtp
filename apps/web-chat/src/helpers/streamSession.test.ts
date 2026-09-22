import { describe, expect, it, vi } from "vitest";
import { createStreamSession } from "./streamSession";

describe("stream session", () => {
  it("does not open streams when cleanup runs during sync", async () => {
    const pendingSync = Promise.withResolvers<undefined>();
    const open = vi.fn(async () => vi.fn());
    const session = createStreamSession([open]);
    const setup = session.start(() => pendingSync.promise);

    session.stop();
    pendingSync.resolve(undefined);
    await setup;

    expect(open).not.toHaveBeenCalled();
  });

  it("closes a stream that opens after cleanup", async () => {
    const pendingStream = Promise.withResolvers<() => void>();
    const opened = Promise.withResolvers<undefined>();
    const close = vi.fn();
    const openNext = vi.fn(async () => vi.fn());
    const session = createStreamSession([
      () => {
        opened.resolve(undefined);
        return pendingStream.promise;
      },
      openNext,
    ]);
    const setup = session.start(() => Promise.resolve());
    await opened.promise;

    session.stop();
    pendingStream.resolve(close);
    await setup;

    expect(close).toHaveBeenCalledOnce();
    expect(openNext).not.toHaveBeenCalled();
  });

  it("closes established and pending streams on cleanup", async () => {
    const closeFirst = vi.fn();
    const closeSecond = vi.fn();
    const pendingStream = Promise.withResolvers<() => void>();
    const openingSecond = Promise.withResolvers<undefined>();
    const session = createStreamSession([
      () => Promise.resolve(closeFirst),
      () => {
        openingSecond.resolve(undefined);
        return pendingStream.promise;
      },
    ]);
    const setup = session.start(() => Promise.resolve());
    await openingSecond.promise;

    session.stop();
    expect(closeFirst).toHaveBeenCalledOnce();
    pendingStream.resolve(closeSecond);
    await setup;
    session.stop();

    expect(closeFirst).toHaveBeenCalledOnce();
    expect(closeSecond).toHaveBeenCalledOnce();
  });

  it("does not replace the latest refresh with an older pending setup", async () => {
    const oldSync = Promise.withResolvers<undefined>();
    const close = vi.fn();
    const open = vi.fn(() => Promise.resolve(close));
    const session = createStreamSession([open]);
    const oldSetup = session.start(() => oldSync.promise);

    await session.start(() => Promise.resolve());
    oldSync.resolve(undefined);
    await oldSetup;

    expect(open).toHaveBeenCalledOnce();
    expect(close).not.toHaveBeenCalled();
    session.stop();
    expect(close).toHaveBeenCalledOnce();
  });

  it("closes a partial setup when another stream fails", async () => {
    const close = vi.fn();
    const error = new Error("stream setup failed");
    const session = createStreamSession([
      () => Promise.resolve(close),
      () => Promise.reject(error),
    ]);

    await expect(session.start(() => Promise.resolve())).rejects.toBe(error);
    expect(close).toHaveBeenCalledOnce();
    session.stop();
    expect(close).toHaveBeenCalledOnce();
  });

  it("keeps a newer session open when an older setup fails", async () => {
    const oldSync = Promise.withResolvers<undefined>();
    const close = vi.fn();
    const error = new Error("old sync failed");
    const session = createStreamSession([() => Promise.resolve(close)]);
    const oldSetup = session.start(() => oldSync.promise);
    const failure = expect(oldSetup).rejects.toBe(error);

    await session.start(() => Promise.resolve());
    oldSync.reject(error);
    await failure;

    expect(close).not.toHaveBeenCalled();
    session.stop();
    expect(close).toHaveBeenCalledOnce();
  });
});
