import type {
  AuthConfiguration,
  Backend,
  LimitsConfiguration,
  MlsConfiguration,
  RetentionConfiguration,
  ServerConfiguration,
  ServerConfigurationError,
  SigningKeyDescription,
} from "@xmtp/node-sdk";
import {
  AuthRequiredError,
  BackendMismatchError,
  ChainNotAcceptedError,
  Client,
  ClientVersionTooOldError,
  ConfigurationInvalidError,
  ConfigurationUnavailableError,
  toServerConfigurationError,
} from "@xmtp/node-sdk";

// Every published value is below 2^53, so the whole object reads as `number`
// and never `bigint` (spec 006 §7).
export function checkServerConfiguration(client: Client): void {
  const configuration: ServerConfiguration = client.serverConfiguration();
  const identifier: string = configuration.identifier;
  const serverVersion: string = configuration.serverVersion;
  const minLibxmtpVersion: string = configuration.minLibxmtpVersion;
  const chains: string[] = configuration.smartContractWalletChains;

  const auth: AuthConfiguration = configuration.auth;
  const enabled: boolean = auth.enabled;
  const keys: SigningKeyDescription[] = auth.keys;
  const kid: string = keys[0].kid;
  const alg: string = keys[0].alg;
  const audiences: string[] = auth.audiences;
  const issuers: string[] = auth.issuers;
  const requiredScopes: string[] = auth.requiredScopes;

  const retention: RetentionConfiguration = configuration.retention;
  const retentionSeconds: number[] = [
    retention.groupMessageSeconds,
    retention.welcomeSeconds,
    retention.keyPackageSeconds,
  ];

  const limits: LimitsConfiguration = configuration.limits;
  const limitValues: number[] = [
    limits.maxEnvelopeBytes,
    limits.maxRequestBytes,
    limits.maxResponseBytes,
    limits.maxPublishTopics,
    limits.maxQueryTopics,
    limits.maxQueryLimit,
    limits.defaultQueryLimit,
    limits.maxNewestMetadataTopics,
    limits.maxNewestFullTopics,
    limits.maxUpdateAdds,
    limits.maxUpdateRemoves,
    limits.maxStreamTopics,
    limits.maxStaticTopics,
    limits.maxLookupIdentifiers,
    limits.maxScwSignatures,
    limits.maxIdentityEntries,
    limits.maxUpdateFramesPerSecond,
    limits.maxUpdateBurst,
    limits.maxPingFramesPerSecond,
    limits.maxPingBurst,
  ];

  const mls: MlsConfiguration = configuration.mls;
  const maxGroupMembers: number = mls.maxGroupMembers;
  const maxInstallationsPerInbox: number = mls.maxInstallationsPerInbox;
  const commitLogEnabled: boolean | undefined = mls.commitLogEnabled;

  void [
    identifier,
    serverVersion,
    minLibxmtpVersion,
    chains,
    enabled,
    kid,
    alg,
    audiences,
    issuers,
    requiredScopes,
    retentionSeconds,
    limitValues,
    maxGroupMembers,
    maxInstallationsPerInbox,
    commitLogEnabled,
  ];
}

declare const maxRequestBytes: LimitsConfiguration["maxRequestBytes"];
// @ts-expect-error Every published number is a `number`, never a `bigint`.
export const notABigint: bigint = maxRequestBytes;

export async function checkFetch(
  url: string,
  backend: Backend,
): Promise<ServerConfiguration[]> {
  return [
    await Client.fetchServerConfiguration(url),
    await Client.fetchServerConfiguration({ backendUrl: url }),
    await Client.fetchServerConfiguration(backend),
  ];
}

export async function checkRefresh(client: Client): Promise<void> {
  const refreshed: ServerConfiguration =
    await client.refreshServerConfiguration();
  void refreshed;
}

// The six failures of CFG-083 are distinct classes with one shared base.
export function checkErrors(error: unknown): string | undefined {
  const typed: ServerConfigurationError | undefined =
    toServerConfigurationError(error);
  if (error instanceof ConfigurationUnavailableError) return error.code;
  if (error instanceof ConfigurationInvalidError) return error.code;
  if (error instanceof BackendMismatchError) return error.code;
  if (error instanceof ClientVersionTooOldError) return error.code;
  if (error instanceof AuthRequiredError) return error.code;
  if (error instanceof ChainNotAcceptedError) return error.code;
  return typed?.code;
}
