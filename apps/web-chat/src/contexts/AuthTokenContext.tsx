import type { AuthCallback, Credential } from "@xmtp/browser-sdk";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "react";
import { useSettings } from "@/hooks/useSettings";

/**
 * A pasted token has no expiry the app can read, and the middleware treats
 * expiry only as a refetch trigger: a token the backend rejects is re-requested
 * through the 401 path regardless of what we claim here. Claim a long life so
 * an unexpired token is not re-prompted on a timer.
 */
const TOKEN_LIFETIME_SECONDS = 365 * 24 * 60 * 60;

/**
 * The SDK sends `value` verbatim and expects the scheme to be included.
 * Operators paste tokens both ways, and double-prefixing produces a 401 that
 * looks like a bad token rather than a formatting mistake.
 */
export const withBearerPrefix = (token: string): string => {
  const trimmed = token.trim();
  return /^\S+\s/.test(trimmed) ? trimmed : `Bearer ${trimmed}`;
};

type PendingRequest = {
  /** Set when the backend rejected a token we already supplied. */
  rejected: boolean;
  resolve: (token: string) => void;
};

export type AuthTokenContextValue = {
  /** Passed to the client. Resolves from storage, or prompts when it cannot. */
  authCallback: AuthCallback;
  /** Non-null while the modal should be open. */
  request: PendingRequest | null;
  /** Open the prompt from the settings panel, outside a backend request. */
  promptForToken: () => void;
};

export const AuthTokenContext = createContext<AuthTokenContextValue>({
  authCallback: () =>
    Promise.reject(new Error("AuthTokenProvider not available")),
  request: null,
  promptForToken: () => {},
});

export const AuthTokenProvider: React.FC<React.PropsWithChildren> = ({
  children,
}) => {
  const { authToken, setAuthToken } = useSettings();
  const [request, setRequest] = useState<PendingRequest | null>(null);
  // The value last handed to the SDK. A second ask for the same value means the
  // backend rejected it, so prompt instead of resubmitting a known-bad token.
  const suppliedRef = useRef<string | null>(null);
  // Read inside the callback so a token saved after the client was built is
  // picked up without rebuilding the callback identity.
  const authTokenRef = useRef(authToken);
  authTokenRef.current = authToken;

  const credentialFor = useCallback(
    (token: string): Credential => ({
      value: withBearerPrefix(token),
      expiresAtSeconds: Math.floor(Date.now() / 1000) + TOKEN_LIFETIME_SECONDS,
    }),
    [],
  );

  const authCallback = useCallback<AuthCallback>(() => {
    const stored = authTokenRef.current.trim();
    const alreadyRejected = stored !== "" && suppliedRef.current === stored;
    if (stored !== "" && !alreadyRejected) {
      suppliedRef.current = stored;
      return Promise.resolve(credentialFor(stored));
    }
    // Hold the promise until the user submits. Rejecting here would fail the
    // request that triggered this and, repeated, reach the backend's
    // consecutive-failure lockout; the middleware is built to await instead.
    return new Promise<Credential>((resolve) => {
      setRequest({
        rejected: alreadyRejected,
        resolve: (token: string) => {
          suppliedRef.current = token.trim();
          setAuthToken(token.trim());
          setRequest(null);
          resolve(credentialFor(token));
        },
      });
    });
  }, [credentialFor, setAuthToken]);

  const promptForToken = useCallback(() => {
    setRequest(
      (current) =>
        current ?? {
          rejected: false,
          resolve: (token: string) => {
            // Entered ahead of any backend request, so there is no pending
            // credential to satisfy. Clear the rejection memo so the next ask
            // uses this token.
            suppliedRef.current = null;
            setAuthToken(token.trim());
            setRequest(null);
          },
        },
    );
  }, [setAuthToken]);

  const value = useMemo(
    () => ({ authCallback, request, promptForToken }),
    [authCallback, request, promptForToken],
  );

  return (
    <AuthTokenContext.Provider value={value}>
      {children}
    </AuthTokenContext.Provider>
  );
};

export const useAuthToken = () => useContext(AuthTokenContext);
