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

/**
 * The middleware asks for a credential before the first request on every
 * client, whether or not the deployment requires one, because a client cannot
 * know until it has asked. Answering the first ask with an empty credential
 * lets an unauthenticated backend connect untouched; a deployment that does
 * require auth rejects it and asks again, which is when we prompt.
 */
const EMPTY_TOKEN_PROBE = "";

export type AuthTokenRequest = {
  /** Set when the backend rejected a token we already supplied. */
  rejected: boolean;
  /** Resolves every callback waiting on this prompt. */
  resolve: (token: string) => void;
};

export type AuthTokenContextValue = {
  /**
   * Builds a callback for one consumer. Each SDK client keeps its own
   * credential cache, so each gets its own memo of what it has offered:
   * a memo shared across clients would read a fresh client's first ask as a
   * rejection of a token that client never sent.
   */
  createAuthCallback: () => AuthCallback;
  /** Non-null while the modal should be open. */
  request: AuthTokenRequest | null;
  /** Open the prompt from the settings panel, outside a backend request. */
  promptForToken: () => void;
};

export const AuthTokenContext = createContext<AuthTokenContextValue>({
  createAuthCallback: () => () =>
    Promise.reject(new Error("AuthTokenProvider not available")),
  request: null,
  promptForToken: () => {},
});

export const AuthTokenProvider: React.FC<React.PropsWithChildren> = ({
  children,
}) => {
  const { authToken, setAuthToken } = useSettings();
  const [request, setRequest] = useState<AuthTokenRequest | null>(null);
  // Read inside the callback so a token saved after the client was built is
  // picked up without rebuilding the callback identity.
  const authTokenRef = useRef(authToken);
  authTokenRef.current = authToken;
  // Every callback waiting on the open prompt. Concurrent asks — a reconnect
  // and an inbox tools query, say — must all be answered by one submission;
  // keeping a single resolver would strand every ask but the newest.
  const waitingRef = useRef<((token: string) => void)[]>([]);
  // Bumped whenever the user submits a token. A consumer's memo is discarded
  // when it is stale, so a token entered after a rejection is offered to every
  // client rather than being treated as already refused.
  const generationRef = useRef(0);

  const credentialFor = useCallback(
    (token: string): Credential => ({
      value: token === "" ? "" : withBearerPrefix(token),
      expiresAtSeconds: Math.floor(Date.now() / 1000) + TOKEN_LIFETIME_SECONDS,
    }),
    [],
  );

  const openPrompt = useCallback(
    (rejected: boolean) => {
      setRequest(
        (current) =>
          current ?? {
            rejected,
            resolve: (token: string) => {
              const trimmed = token.trim();
              generationRef.current += 1;
              setAuthToken(trimmed);
              setRequest(null);
              const waiting = waitingRef.current;
              waitingRef.current = [];
              for (const resolve of waiting) resolve(trimmed);
            },
          },
      );
      // A prompt already open for a first ask becomes a rejection notice when a
      // later ask proves the supplied credential was refused.
      if (rejected) {
        setRequest((current) =>
          current && !current.rejected
            ? { ...current, rejected: true }
            : current,
        );
      }
    },
    [setAuthToken],
  );

  const createAuthCallback = useCallback<() => AuthCallback>(() => {
    // Per consumer, matching one SDK credential cache.
    let supplied = new Set<string>();
    let generation = generationRef.current;

    return () => {
      if (generation !== generationRef.current) {
        // A newer token exists than anything this consumer has offered.
        supplied = new Set<string>();
        generation = generationRef.current;
      }
      const stored = authTokenRef.current.trim();

      // A token this consumer has not yet offered: hand it over and wait to
      // see whether the backend accepts it.
      if (stored !== "" && !supplied.has(stored)) {
        supplied.add(stored);
        return Promise.resolve(credentialFor(stored));
      }

      // No token stored, and this consumer has not probed yet. The deployment
      // may not want one at all, so answer with an empty credential rather
      // than interrupting the user. A deployment that needs auth rejects this
      // and asks again.
      if (stored === "" && !supplied.has(EMPTY_TOKEN_PROBE)) {
        supplied.add(EMPTY_TOKEN_PROBE);
        return Promise.resolve(credentialFor(EMPTY_TOKEN_PROBE));
      }

      // Everything this consumer has has been offered and refused. Hold the
      // promise until the user submits: rejecting would fail the request that
      // triggered this and, repeated, reach the backend's consecutive-failure
      // lockout.
      return new Promise<Credential>((resolve) => {
        waitingRef.current.push((token: string) => {
          resolve(credentialFor(token));
        });
        openPrompt(stored !== "");
      });
    };
  }, [credentialFor, openPrompt]);

  const promptForToken = useCallback(() => {
    openPrompt(false);
  }, [openPrompt]);

  const value = useMemo(
    () => ({ createAuthCallback, request, promptForToken }),
    [createAuthCallback, request, promptForToken],
  );

  return (
    <AuthTokenContext.Provider value={value}>
      {children}
    </AuthTokenContext.Provider>
  );
};

export const useAuthToken = () => useContext(AuthTokenContext);
