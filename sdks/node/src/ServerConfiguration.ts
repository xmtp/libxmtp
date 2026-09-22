/**
 * Typed failures for the deployment configuration a backend publishes
 * (spec 006 §7).
 *
 * The bindings report every one of these as an `Error` whose message begins
 * with `[ClientError::<Variant>]`. Matching on that string is what this module
 * removes: each of the six documented codes becomes its own class, so an app
 * writes `error instanceof BackendMismatchError` instead of a regular
 * expression over a message that may be reworded.
 *
 * Every class keeps the binding's message and the original error as `cause`,
 * so the identifiers, versions, scopes, or chain the core reported are still
 * readable for diagnostics.
 */

/**
 * Base class of the six configuration failures. Catch this to handle any of
 * them; catch a subclass to handle one.
 */
export class ServerConfigurationError extends Error {
  constructor(
    /** The stable code the bindings reported, e.g. `ClientError::AuthRequired`. */
    readonly code: string,
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "ServerConfigurationError";
  }
}

/**
 * The configuration could not be read: the deployment did not answer, answered
 * `UNIMPLEMENTED` because it predates spec 006, or the copy could not be
 * stored.
 */
export class ConfigurationUnavailableError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ConfigurationUnavailable", message, options);
    this.name = "ConfigurationUnavailableError";
  }
}

/**
 * The deployment answered with a configuration no client can use: an empty or
 * malformed identifier, an unparseable minimum version, or a chain that is not
 * a CAIP-2 identifier. Nothing is stored.
 */
export class ConfigurationInvalidError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ConfigurationInvalid", message, options);
    this.name = "ConfigurationInvalidError";
  }
}

/**
 * This database is bound to a different deployment. The identifier the backend
 * published is not the one the database was created against. Recovery is a
 * database created for the deployment the app now uses.
 */
export class BackendMismatchError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::BackendMismatch", message, options);
    this.name = "BackendMismatchError";
  }
}

/** The deployment requires a newer libxmtp than this SDK is built on. */
export class ClientVersionTooOldError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ClientVersionTooOld", message, options);
    this.name = "ClientVersionTooOldError";
  }
}

/**
 * The deployment requires authentication and no credential source was
 * configured. The message carries the scopes it requires.
 */
export class AuthRequiredError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::AuthRequired", message, options);
    this.name = "AuthRequiredError";
  }
}

/**
 * A smart contract wallet signature names a chain the deployment does not
 * verify on. Raised before any network call, and only for signatures the app
 * supplied.
 */
export class ChainNotAcceptedError extends ServerConfigurationError {
  constructor(message: string, options?: ErrorOptions) {
    super("ClientError::ChainNotAccepted", message, options);
    this.name = "ChainNotAcceptedError";
  }
}

const configurationCode = /^\[(ClientError::[A-Za-z]+)\]\s*([\s\S]*)$/;

// implements: CONF-064
const build = (
  code: string,
  message: string,
  options: ErrorOptions,
): ServerConfigurationError | undefined => {
  switch (code) {
    case "ClientError::ConfigurationUnavailable":
      return new ConfigurationUnavailableError(message, options);
    case "ClientError::ConfigurationInvalid":
      return new ConfigurationInvalidError(message, options);
    case "ClientError::BackendMismatch":
      return new BackendMismatchError(message, options);
    case "ClientError::ClientVersionTooOld":
      return new ClientVersionTooOldError(message, options);
    case "ClientError::AuthRequired":
      return new AuthRequiredError(message, options);
    case "ClientError::ChainNotAccepted":
      return new ChainNotAcceptedError(message, options);
    default:
      return undefined;
  }
};

/**
 * Classifies an error from any SDK call as one of the six configuration
 * failures, or `undefined` when it is something else.
 *
 * Use this where the SDK hands back the raw error, for instance a
 * `ChainNotAcceptedError` raised deep inside a call that supplies a smart
 * contract wallet signature.
 */
export const toServerConfigurationError = (
  error: unknown,
): ServerConfigurationError | undefined => {
  if (!(error instanceof Error)) return undefined;
  const match = configurationCode.exec(error.message);
  if (!match) return undefined;
  return build(match[1], match[2], { cause: error });
};

/**
 * Rethrows an error as its typed configuration failure, or unchanged when it
 * is not one.
 */
export const throwServerConfigurationError = (error: unknown): never => {
  throw toServerConfigurationError(error) ?? error;
};
