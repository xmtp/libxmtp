// SDK client creation, the backend probe and SDK error mapping.

import {
  Client,
  type ClientOptions,
  XmtpError,
  localSignerFromPrivateKey,
} from "@xmtp/react-native-sdk";

import { type BackendUrl, normalizeBackendUrl } from "@/core/backendUrl.ts";
import { bearerCredential } from "@/core/bearer.ts";
import type { HandleError } from "@/core/handles.ts";
import type {
  ClientFactory,
  OpenRequest,
  SessionClient,
} from "@/core/sessionController.ts";
import { SessionCopy, SessionError } from "@/core/sessionError.ts";
import { launchEnvironment } from "@/platform/messengerPlatform";

/** An opened SDK client with the fields the session machine reads. */
export interface OpenedClient extends SessionClient {
  readonly client: Client;
}

/** True when the launch allowed private attachment URLs and the backend is loopback. */
export function allowsPrivateAttachments(backend: BackendUrl): boolean {
  return (
    launchEnvironment().allowPrivateAttachments && backend.allowPrivateNetwork
  );
}

/** The client options for a profile. */
export function sessionOptions(request: OpenRequest): ClientOptions {
  const backend = normalizeBackendUrl(request.profile.url);
  const read = request.credential;
  return {
    backend: {
      url: backend.value,
      // Attach a credential source only when a token is stored: the SDK asks
      // for it before the first request even when the server needs none.
      ...(read === undefined
        ? {}
        : {
            credentials: {
              credential: async () => bearerCredential(await read()),
            },
          }),
    },
    storage: {
      location: {
        dbPath: request.paths.database,
        attachmentsDir: request.paths.attachments,
      },
      encryptionKey: request.databaseKey,
    },
    // A known inbox reopens with stored state even when the server is down.
    allowOffline: request.profile.inboxId !== undefined,
    attachments: { allowPrivateNetwork: allowsPrivateAttachments(backend) },
  };
}

/** The Connect copy for an SDK open failure. */
export function connectionError(error: unknown): Error {
  if (error instanceof XmtpError.CredentialRejected) {
    return SessionError.message(SessionCopy.tokenRejected);
  }
  return error instanceof Error ? error : new Error(String(error));
}

/** Opens profiles: `Client.build` for a known identity, `Client.create` otherwise. */
export class SdkClientFactory implements ClientFactory<OpenedClient> {
  async open(request: OpenRequest): Promise<OpenedClient> {
    const options = sessionOptions(request);
    const { inboxId, identity } = request.profile;
    let client: Client;
    try {
      client =
        inboxId !== undefined && identity !== undefined
          ? await Client.build(
              { identifier: identity, kind: "ethereum" },
              options,
              inboxId,
            )
          : await Client.create(
              await localSignerFromPrivateKey(request.wallet),
              options,
            );
    } catch (error) {
      throw connectionError(error);
    }
    return {
      client,
      inboxId: client.inboxId,
      identity: client.identity.identifier,
      end: () => client.end(),
    };
  }
}

export interface BackendProbeResult {
  /** The server requires an access token (`auth.enabled`). */
  readonly requiresToken: boolean;
}

/**
 * Checks that the server answers its configuration endpoint. Sends no
 * credentials. Any failure means "unreachable" to the Connect screen.
 */
export async function probeBackend(
  backend: BackendUrl,
): Promise<BackendProbeResult> {
  const configuration = await Client.fetchServerConfiguration({
    url: backend.value,
  });
  return { requiresToken: configuration.auth.enabled };
}

/** The handle failure for an SDK error. */
export function handleError(error: unknown): HandleError {
  if (error instanceof XmtpError.HandleTaken) return "taken";
  if (error instanceof XmtpError.HandleNotAllowed) return "notAllowed";
  if (error instanceof XmtpError.HandleTooSoon) return "tooSoon";
  if (error instanceof XmtpError.HandleLimitReached) return "limitReached";
  if (error instanceof XmtpError.InvalidArgument) return "invalidArgument";
  return "failed";
}

/** True when a lookup failed because the identity has no inbox. */
export function isIdentityNotFound(error: unknown): boolean {
  return error instanceof XmtpError.IdentityNotFound;
}
