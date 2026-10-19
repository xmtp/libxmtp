import {
  createContext,
  createElement,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactElement,
} from "react";

import {
  asError,
  ClientLifetime,
  isCancelled,
  reportError,
} from "./lifetime.js";
import type { XmtpContextValue, XmtpProviderProps } from "./types.js";

export type {
  InitializeClientOptions,
  XmtpContextValue,
  XmtpProviderProps,
} from "./types.js";

export const XmtpContext = createContext<XmtpContextValue | undefined>(
  undefined,
);

export function XmtpProvider({
  children,
  client,
  onError,
}: XmtpProviderProps): ReactElement {
  const [lifetime] = useState(() => new ClientLifetime(client));
  const snapshot = useSyncExternalStore(
    lifetime.subscribe,
    lifetime.getSnapshot,
    lifetime.getSnapshot,
  );
  useLayoutEffect(() => {
    lifetime.mount();
    return () => lifetime.unmount();
  }, [lifetime]);
  useLayoutEffect(() => lifetime.setErrorHandler(onError), [lifetime, onError]);
  useLayoutEffect(() => lifetime.replace(client), [lifetime, client]);
  const value = useMemo<XmtpContextValue>(
    () => ({
      ...snapshot,
      initialize: lifetime.initialize,
      disconnect: lifetime.disconnect,
    }),
    [snapshot, lifetime],
  );
  return createElement(XmtpContext.Provider, { value }, children);
}

export function useXmtp(): XmtpContextValue {
  const context = useContext(XmtpContext);
  if (!context)
    throw new TypeError("XMTP hooks must be used within XmtpProvider");
  return context;
}

export function useClient(onError?: (error: Error) => void): XmtpContextValue {
  const context = useXmtp();
  const observer = useRef({ active: false, onError });
  useLayoutEffect(() => {
    observer.current = { active: true, onError };
    return () => {
      observer.current.active = false;
    };
  }, [onError]);
  const commands = useMemo(() => {
    const failed = (cause: unknown): never => {
      const error = asError(cause);
      if (
        observer.current.active &&
        observer.current.onError &&
        !isCancelled(error)
      ) {
        reportError(error, observer.current.onError);
      }
      throw error;
    };
    return {
      initialize: (options: Parameters<XmtpContextValue["initialize"]>[0]) =>
        context.initialize(options).catch(failed),
      disconnect: () => context.disconnect().catch(failed),
    };
  }, [context.initialize, context.disconnect]);
  return useMemo(() => ({ ...context, ...commands }), [context, commands]);
}
