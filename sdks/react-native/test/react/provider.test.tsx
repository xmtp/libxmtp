import { Client, XmtpError } from "@xmtp/react-native-sdk";
import {
  act,
  createElement,
  StrictMode,
  useLayoutEffect,
  type ReactElement,
} from "react";
import { create, type ReactTestRenderer } from "react-test-renderer";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
  type MockInstance,
} from "vitest";

import {
  useClient,
  XmtpProvider,
  type InitializeClientOptions,
  type XmtpContextValue,
  type XmtpProviderProps,
} from "../../src/react/index.js";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((accept, decline) => {
    resolve = accept;
    reject = decline;
  });
  return { promise, resolve, reject };
}

// The fixture supplies only the native resource boundary. The real provider
// decides who owns it, when it is exposed, and when end is called.
function resource(end?: () => Promise<void>) {
  const state = { closed: false, ends: 0 };
  const client = {
    async end() {
      state.ends += 1;
      await end?.();
      state.closed = true;
    },
  } as Client;
  return { client, state };
}

// Factory arguments are opaque to these lifetime tests; their public types
// come from the generated root, and the native factory is the only stub.
const input = { signer: {}, options: {} } as InitializeClientOptions;
let roots: ReactTestRenderer[];
let factory: MockInstance<typeof Client.create>;
let diagnostic: MockInstance<typeof console.error>;

beforeEach(() => {
  roots = [];
  factory = vi
    .spyOn(Client, "create")
    .mockRejectedValue(new Error("unexpected native creation"));
  diagnostic = vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(async () => {
  await act(async () => roots.forEach((root) => root.unmount()));
  vi.restoreAllMocks();
});

async function render(
  props: Omit<XmtpProviderProps, "children"> = {},
  hookError?: (error: Error) => void,
  onLayout?: (context: XmtpContextValue) => void,
) {
  const values: XmtpContextValue[] = [];
  function Consumer({ index }: { index: number }) {
    values[index] = useClient(hookError);
    useLayoutEffect(() => {
      if (index === 0) onLayout?.(values[index]);
    });
    return null;
  }
  function tree(next: Omit<XmtpProviderProps, "children">) {
    return createElement(XmtpProvider, {
      ...next,
      children: [
        createElement(Consumer, { key: 0, index: 0 }),
        createElement(Consumer, { key: 1, index: 1 }),
      ],
    });
  }
  let root!: ReactTestRenderer;
  await act(async () => {
    root = create(tree(props));
  });
  roots.push(root);
  return {
    values,
    async update(next: Omit<XmtpProviderProps, "children">) {
      await act(async () => root.update(tree(next)));
    },
    async unmount() {
      await act(async () => root.unmount());
    },
  };
}

function observe<T>(promise: Promise<T>) {
  const result: { value?: T; error?: unknown; settled: boolean } = {
    settled: false,
  };
  void promise.then(
    (value) => {
      result.value = value;
      result.settled = true;
    },
    (error: unknown) => {
      result.error = error;
      result.settled = true;
    },
  );
  return result;
}

async function initialize(
  view: Awaited<ReturnType<typeof render>>,
  client: Client,
) {
  factory.mockResolvedValueOnce(client);
  await act(async () => {
    await view.values[0].initialize(input);
  });
}

describe("React client lifetime", () => {
  it("shares creation across consumers, preserves failures, and retries with loading reset", async () => {
    const onError = vi.fn();
    const view = await render({}, onError);
    const first = deferred<Client>();
    factory.mockReturnValueOnce(first.promise);
    let one!: ReturnType<typeof observe<Client>>;
    let two!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      one = observe(view.values[0].initialize(input));
      two = observe(view.values[1].initialize(input));
    });
    expect(factory).toHaveBeenCalledTimes(1);
    expect(view.values.map((value) => value.isLoading)).toEqual([true, true]);
    const failure = new XmtpError.StorageBusy({
      code: "StorageBusy",
      category: "storage",
      retryable: true,
      message: "busy",
    });
    await act(async () => first.reject(failure));
    expect(one.error).toBe(failure);
    expect(two.error).toBe(failure);
    expect(view.values[0]).toMatchObject({
      error: failure,
      client: null,
      isLoading: false,
    });
    expect(onError.mock.calls).toEqual([[failure], [failure]]);

    const cause = { privateMessage: "opaque rejection" };
    factory.mockRejectedValueOnce(cause);
    let nonError!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      nonError = observe(view.values[0].initialize(input));
    });
    expect(nonError.error).toBeInstanceOf(Error);
    expect(nonError.error).toHaveProperty("cause", cause);
    expect(view.values[0].isLoading).toBe(false);

    const owned = resource();
    await initialize(view, owned.client);
    expect(factory).toHaveBeenCalledTimes(3);
    expect(view.values[0]).toMatchObject({
      client: owned.client,
      error: null,
      isLoading: false,
    });
    await act(async () =>
      expect(await view.values[1].initialize(input)).toBe(owned.client),
    );
    expect(factory).toHaveBeenCalledTimes(3);
  });

  it("reacts to borrowed identity changes without closing them or reattaching after disconnect", async () => {
    const a = resource();
    const b = resource();
    const view = await render({ client: a.client });
    expect(view.values[0].client).toBe(a.client);
    await act(async () =>
      expect(await view.values[0].initialize(input)).toBe(a.client),
    );
    await view.update({ client: b.client });
    expect(view.values[0].client).toBe(b.client);
    await act(async () => view.values[0].disconnect());
    await view.update({ client: b.client });
    expect(view.values[0].client).toBeNull();
    await view.unmount();
    expect(a.state).toEqual({ closed: false, ends: 0 });
    expect(b.state).toEqual({ closed: false, ends: 0 });
    expect(factory).not.toHaveBeenCalled();
  });

  it("retains ownership when its client is fed back and rejects closing and closed aliases", async () => {
    const closing = deferred<void>();
    const owned = resource(() => closing.promise);
    const view = await render();
    await initialize(view, owned.client);
    await view.update({ client: owned.client });
    expect(view.values[0].client).toBe(owned.client);
    expect(owned.state).toEqual({ closed: false, ends: 0 });
    await act(async () =>
      expect(await view.values[0].initialize(input)).toBe(owned.client),
    );
    let disconnected!: ReturnType<typeof observe<void>>;
    await act(async () => {
      disconnected = observe(view.values[0].disconnect());
    });
    expect(view.values[0].client).toBeNull();
    expect(disconnected.settled).toBe(false);
    await view.update({ client: null });
    await view.update({ client: owned.client });
    expect(view.values[0].client).toBeNull();
    expect(view.values[0].error).toBeInstanceOf(TypeError);
    await act(async () => closing.resolve());
    expect(disconnected.settled).toBe(true);
    expect(owned.state).toEqual({ closed: true, ends: 1 });
    await view.update({ client: null });
    await view.update({ client: owned.client });
    expect(view.values[0].client).toBeNull();
    expect(view.values[0].error).toBeInstanceOf(TypeError);
    expect(owned.state.ends).toBe(1);
  });

  it.each(["disconnect", "replace", "unmount"] as const)(
    "%s cancels a held wallet before release and closes its late client",
    async (action) => {
      const wallet = deferred<Client>();
      const late = resource();
      const borrowed = resource();
      const onError = vi.fn();
      const view = await render({}, onError);
      factory.mockReturnValueOnce(wallet.promise);
      let pending!: ReturnType<typeof observe<Client>>;
      await act(async () => {
        pending = observe(view.values[0].initialize(input));
      });
      expect(factory).toHaveBeenCalledTimes(1);
      if (action === "disconnect")
        await act(async () => view.values[0].disconnect());
      if (action === "replace") await view.update({ client: borrowed.client });
      if (action === "unmount") await view.unmount();
      expect(pending.settled).toBe(true);
      expect(pending.error).toBeInstanceOf(XmtpError.Cancelled);
      expect(onError).not.toHaveBeenCalled();
      if (action !== "unmount") {
        expect(view.values[0].isLoading).toBe(false);
        expect(view.values[0].error).toBeNull();
      }
      const next = resource();
      if (action === "disconnect") {
        // An abandoned wallet cannot hold the next provider generation hostage.
        await initialize(view, next.client);
        expect(view.values[0].client).toBe(next.client);
      }
      await act(async () => wallet.resolve(late.client));
      expect(late.state).toEqual({ closed: true, ends: 1 });
      expect(borrowed.state.closed).toBe(false);
      if (action === "replace")
        expect(view.values[0].client).toBe(borrowed.client);
      if (action === "disconnect")
        expect(view.values[0].client).toBe(next.client);
    },
  );

  it("retains failed cleanup, blocks creation, and waits for an explicit cleanup retry", async () => {
    const failure = new XmtpError.StorageBusy({
      code: "StorageBusy",
      category: "storage",
      retryable: true,
      message: "close failed",
    });
    const retry = deferred<void>();
    let ends = 0;
    const old = resource(() =>
      ++ends === 1 ? Promise.reject(failure) : retry.promise,
    );
    const onError = vi.fn(() => {
      throw new Error("observer failed");
    });
    const view = await render({}, onError);
    await initialize(view, old.client);
    let disconnect!: ReturnType<typeof observe<void>>;
    await act(async () => {
      disconnect = observe(view.values[0].disconnect());
    });
    expect(disconnect.error).toBe(failure);
    expect(view.values[0]).toMatchObject({
      client: null,
      error: failure,
      isLoading: false,
    });
    expect(old.state).toEqual({ closed: false, ends: 1 });
    let blocked!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      blocked = observe(view.values[0].initialize(input));
    });
    expect(blocked.error).toBe(failure);
    expect(factory).toHaveBeenCalledTimes(1);
    expect(onError.mock.calls).toEqual([[failure], [failure]]);
    const next = resource();
    factory.mockResolvedValueOnce(next.client);
    let created!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      disconnect = observe(view.values[0].disconnect());
      created = observe(view.values[0].initialize(input));
    });
    expect(view.values[0].isLoading).toBe(true);
    expect(factory).toHaveBeenCalledTimes(1);
    await act(async () => retry.resolve());
    expect(disconnect.settled).toBe(true);
    expect(disconnect.error).toBeUndefined();
    expect(created.value).toBe(next.client);
    expect(old.state).toEqual({ closed: true, ends: 2 });
    expect(view.values[0]).toMatchObject({
      client: next.client,
      error: null,
      isLoading: false,
    });
  });

  it("waits for owned cleanup before publishing the latest borrowed replacement", async () => {
    const closing = deferred<void>();
    const owned = resource(() => closing.promise);
    const a = resource();
    const b = resource();
    let initializeOnLayout = false;
    let duringReplacement!: ReturnType<typeof observe<Client>>;
    const view = await render({}, undefined, (context) => {
      if (initializeOnLayout) {
        initializeOnLayout = false;
        duringReplacement = observe(context.initialize(input));
      }
    });
    await initialize(view, owned.client);
    initializeOnLayout = true;
    await view.update({ client: a.client });
    expect(duringReplacement.error).toBeInstanceOf(XmtpError.Cancelled);
    expect(duringReplacement.value).toBeUndefined();
    await view.update({ client: b.client });
    expect(view.values[0].client).toBeNull();
    let pending!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      pending = observe(view.values[0].initialize(input));
    });
    expect(view.values[0].isLoading).toBe(true);
    expect(factory).toHaveBeenCalledTimes(1);
    await act(async () => closing.resolve());
    expect(view.values[0]).toMatchObject({
      client: b.client,
      isLoading: false,
    });
    expect(pending.value).toBe(b.client);
    expect(owned.state).toEqual({ closed: true, ends: 1 });
    expect(a.state.ends + b.state.ends).toBe(0);
  });

  it("rejects publication when an abandoned owner's cleanup fails during a newer create", async () => {
    const firstWallet = deferred<Client>();
    const nextWallet = deferred<Client>();
    const firstClose = deferred<void>();
    const original = new Error("abandoned owner cleanup failed");
    const secondary = new Error("unpublished owner cleanup failed");
    let firstEnds = 0;
    let nextEnds = 0;
    const abandoned = resource(() =>
      ++firstEnds === 1 ? firstClose.promise : Promise.resolve(),
    );
    const unpublished = resource(() =>
      ++nextEnds === 1 ? Promise.reject(secondary) : Promise.resolve(),
    );
    const onError = vi.fn();
    const view = await render({ onError });
    factory
      .mockReturnValueOnce(firstWallet.promise)
      .mockReturnValueOnce(nextWallet.promise);
    let first!: ReturnType<typeof observe<Client>>;
    let next!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      first = observe(view.values[0].initialize(input));
    });
    await act(async () => view.values[0].disconnect());
    expect(first.error).toBeInstanceOf(XmtpError.Cancelled);
    await act(async () => {
      next = observe(view.values[0].initialize(input));
    });
    expect(factory).toHaveBeenCalledTimes(2);
    await act(async () => firstWallet.resolve(abandoned.client));
    await act(async () => firstClose.reject(original));
    await act(async () => nextWallet.resolve(unpublished.client));
    expect(next.error).toBe(original);
    expect(view.values[0]).toMatchObject({
      client: null,
      error: original,
      isLoading: false,
    });
    expect(abandoned.state).toEqual({ closed: false, ends: 1 });
    expect(unpublished.state).toEqual({ closed: false, ends: 1 });
    expect(onError.mock.calls).toEqual([[original], [secondary]]);
    // Neither failed owner is forgotten or silently retried by initialization.
    let blocked!: ReturnType<typeof observe<Client>>;
    await act(async () => {
      blocked = observe(view.values[0].initialize(input));
    });
    expect(blocked.error).toBe(original);
    expect(factory).toHaveBeenCalledTimes(2);
    await act(async () => view.values[0].disconnect());
    expect(abandoned.state).toEqual({ closed: true, ends: 2 });
    expect(unpublished.state).toEqual({ closed: true, ends: 2 });
    const repaired = resource();
    await initialize(view, repaired.client);
    expect(view.values[0]).toMatchObject({
      client: repaired.client,
      error: null,
    });
  });

  it("reports cleanup after unmount through the provider and redacts the fallback", async () => {
    const ending = deferred<void>();
    const owned = resource(() => ending.promise);
    const providerError = vi.fn(() => {
      throw new Error("observer secret");
    });
    const hookError = vi.fn();
    const view = await render({ onError: providerError }, hookError);
    await initialize(view, owned.client);
    let command!: ReturnType<typeof observe<void>>;
    await act(async () => {
      command = observe(view.values[0].disconnect());
    });
    await view.unmount();
    const failure = new Error("native secret");
    await act(async () => ending.reject(failure));
    expect(command.error).toBe(failure);
    expect(providerError.mock.calls).toEqual([[failure]]);
    expect(hookError).not.toHaveBeenCalled();
    expect(diagnostic.mock.calls.flat().join(" ")).not.toContain("secret");
    expect(diagnostic).toHaveBeenCalledWith(
      "XMTP React client cleanup or error observer failed",
    );

    const wallet = deferred<Client>();
    const late = resource(() => Promise.reject(failure));
    const noHandler = await render();
    factory.mockReturnValueOnce(wallet.promise);
    await act(async () => {
      observe(noHandler.values[0].initialize(input));
    });
    await noHandler.unmount();
    diagnostic.mockClear();
    await act(async () => wallet.resolve(late.client));
    expect(late.state.ends).toBe(1);
    expect(diagnostic.mock.calls).toEqual([
      ["XMTP React client cleanup or error observer failed"],
    ]);
  });

  it("survives StrictMode effect replay without stale publication or orphaned owners", async () => {
    const created: ReturnType<typeof resource>[] = [];
    const outcomes: ReturnType<typeof observe<Client>>[] = [];
    factory.mockImplementation(async () => {
      const owned = resource();
      created.push(owned);
      return owned.client;
    });
    let latest!: XmtpContextValue;
    function Consumer(): ReactElement | null {
      const context = useClient();
      latest = context;
      useLayoutEffect(() => {
        outcomes.push(observe(context.initialize(input)));
      }, [context.initialize]);
      return null;
    }
    let root!: ReactTestRenderer;
    await act(async () => {
      root = create(
        createElement(
          StrictMode,
          null,
          createElement(XmtpProvider, { children: createElement(Consumer) }),
        ),
      );
    });
    roots.push(root);
    expect(created.length).toBeGreaterThan(0);
    const live = created.filter((owned) => !owned.state.closed);
    expect(live).toHaveLength(1);
    expect(latest.client).toBe(live[0].client);
    expect(latest.error).toBeNull();
    expect(latest.isLoading).toBe(false);
    expect(outcomes.every((outcome) => outcome.settled)).toBe(true);
    await act(async () => root.unmount());
    expect(
      created.every((owned) => owned.state.closed && owned.state.ends === 1),
    ).toBe(true);
  });
});
