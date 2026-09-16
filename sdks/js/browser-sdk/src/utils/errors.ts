export class ClientNotInitializedError extends Error {
  constructor() {
    super(
      "Client not initialized, use Client.create or Client.build to create a client",
    );
  }
}

export class SignerUnavailableError extends Error {
  constructor() {
    super(
      "Signer unavailable, use Client.create to create a client with a signer",
    );
  }
}

export class InboxReassignError extends Error {
  constructor() {
    super(
      "Unable to create add account signature text, `allowInboxReassign` must be true",
    );
  }
}

export class AccountAlreadyAssociatedError extends Error {
  constructor(inboxId: string) {
    super(`Account already associated with inbox ${inboxId}`);
  }
}

export class GroupNotFoundError extends Error {
  constructor(groupId: string) {
    super(`Group "${groupId}" not found`);
  }
}

export class StreamNotFoundError extends Error {
  constructor(streamId: string) {
    super(`Stream "${streamId}" not found`);
  }
}

export class StreamFailedError extends Error {
  constructor(retryAttempts: number) {
    const times = `time${retryAttempts !== 1 ? "s" : ""}`;
    super(`Stream failed, retried ${retryAttempts} ${times}`);
  }
}

export class StreamInvalidRetryAttemptsError extends Error {
  constructor() {
    super("Stream retry attempts must be greater than 0");
  }
}

export class OpfsNotInitializedError extends Error {
  constructor() {
    super("OPFS must be initialized before accessing its methods");
  }
}

export class OpfsInitializationError extends Error {
  constructor() {
    super(
      "Failed to initialize OPFS, ensure that there are no other active XMTP clients or Opfs instances",
    );
  }
}

/**
 * Base class for the server configuration failures that spec 006 (CFG-083)
 * requires each SDK to surface as a distinct type.
 *
 * `code` is the binding error code. The WASM bindings set it as a property on
 * the thrown error and also prefix the message with `[<code>] `. Only the
 * message survives the structured clone that carries a worker error back to
 * the main thread, so the prefix is the reliable source of the code.
 */
export class ServerConfigurationError extends Error {
  readonly code: string;

  constructor(code: string, message: string, options?: ErrorOptions) {
    super(message, options);
    this.code = code;
    this.name = "ServerConfigurationError";
  }
}

/** The backend configuration could not be fetched or stored (CFG-041). */
export class ConfigurationUnavailableError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ConfigurationUnavailable", message, options);
    this.name = "ConfigurationUnavailableError";
  }
}

/** The backend published a configuration the client rejects (CFG-044). */
export class ConfigurationInvalidError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ConfigurationInvalid", message, options);
    this.name = "ConfigurationInvalidError";
  }
}

/** The database is bound to a different backend identifier (CFG-051). */
export class BackendMismatchError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::BackendMismatch", message, options);
    this.name = "BackendMismatchError";
  }
}

/** The backend requires a newer libxmtp version (CFG-060, CFG-061). */
export class ClientVersionTooOldError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ClientVersionTooOld", message, options);
    this.name = "ClientVersionTooOldError";
  }
}

/** The backend requires a credential and none was configured (CFG-062). */
export class AuthRequiredError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::AuthRequired", message, options);
    this.name = "AuthRequiredError";
  }
}

/** The backend does not accept the chain of a supplied signature (CFG-069). */
export class ChainNotAcceptedError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ChainNotAccepted", message, options);
    this.name = "ChainNotAcceptedError";
  }
}

type ServerConfigurationErrorConstructor = new (
  message: string,
  options?: ErrorOptions,
) => ServerConfigurationError;

// The keys are the binding error codes this SDK types; a lookup with any other
// code is a miss, so the value type admits `undefined`.
const serverConfigurationErrors: Record<
  string,
  ServerConfigurationErrorConstructor | undefined
> = {
  "ClientError::ConfigurationUnavailable": ConfigurationUnavailableError,
  "ClientError::ConfigurationInvalid": ConfigurationInvalidError,
  "ClientError::BackendMismatch": BackendMismatchError,
  "ClientError::ClientVersionTooOld": ClientVersionTooOldError,
  "ClientError::AuthRequired": AuthRequiredError,
  "ClientError::ChainNotAccepted": ChainNotAcceptedError,
};

const errorCodePattern = /^\[([^\]]+)\]\s?/;

/**
 * Reads the binding error code from an error thrown by the WASM bindings.
 *
 * Prefers the `code` property the bindings set on the main thread and falls
 * back to the `[<code>] ` message prefix, which is all that survives a worker
 * error transfer.
 */
export const getErrorCode = (error: unknown): string | undefined => {
  if (typeof error !== "object" || error === null) return undefined;
  const code: unknown = (error as { code?: unknown }).code;
  if (typeof code === "string" && code !== "") return code;
  const message: unknown = (error as { message?: unknown }).message;
  if (typeof message !== "string") return undefined;
  return errorCodePattern.exec(message)?.[1];
};

/**
 * Maps a binding error onto its typed class (CFG-083).
 *
 * Returns `undefined` when the error carries no code this SDK types, so the
 * caller can rethrow the original error untouched.
 */
export const toServerConfigurationError = (
  error: unknown,
): ServerConfigurationError | undefined => {
  if (error instanceof ServerConfigurationError) return error;
  const code = getErrorCode(error);
  if (code === undefined) return undefined;
  const ServerConfigurationErrorClass = serverConfigurationErrors[code];
  if (ServerConfigurationErrorClass === undefined) return undefined;
  const message: unknown = (error as { message?: unknown }).message;
  const text = typeof message === "string" ? message : code;
  return new ServerConfigurationErrorClass(text.replace(errorCodePattern, ""), {
    cause: error,
  });
};
