import { Client } from "@xmtp/browser-sdk";
import { useEffect, useMemo, useState } from "react";

import { isValidBackendUrl } from "@/helpers/backend";

export type ServerAuthConfig = {
  /** False only when the backend positively says auth is off. */
  required: boolean;
  /** Scopes the backend asks for, shown as a hint when it names any. */
  requiredScopes: string[];
  loading: boolean;
};

/**
 * Asks the backend whether it requires a credential, so the token field is
 * shown only when it matters. This read needs no credential itself (CFG-081).
 *
 * An unreachable or older backend leaves `required` true: hiding the field on a
 * failed read would strand a user on a backend that does need a token, with no
 * way to enter one.
 */
export const useServerAuthConfig = (backendUrl: string): ServerAuthConfig => {
  const request = useMemo(() => ({ backendUrl }), [backendUrl]);
  const [result, setResult] = useState<{
    request: typeof request;
    config: ServerAuthConfig;
  }>();

  useEffect(() => {
    if (!isValidBackendUrl(request.backendUrl)) return;

    // A slow backend must not leave a stale answer on screen, and a response
    // for a previous URL must not overwrite a newer one.
    let active = true;
    Client.fetchServerConfiguration(request.backendUrl)
      .then((configuration) => {
        if (!active) return;
        setResult({
          request,
          config: {
            required: configuration.auth.enabled,
            requiredScopes: configuration.auth.requiredScopes,
            loading: false,
          },
        });
      })
      .catch(() => {
        if (!active) return;
        setResult({
          request,
          config: { required: true, requiredScopes: [], loading: false },
        });
      });

    return () => {
      active = false;
    };
  }, [request]);

  if (!isValidBackendUrl(backendUrl)) {
    return { required: true, requiredScopes: [], loading: false };
  }
  return result?.request === request
    ? result.config
    : { required: true, requiredScopes: [], loading: true };
};
