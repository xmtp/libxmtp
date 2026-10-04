import type {
  AuthConfiguration,
  Backend,
  LimitsConfiguration,
  MlsConfiguration,
  RetentionConfiguration,
  ServerConfiguration,
  SigningKeyDescription,
} from "@xmtp/node-sdk";
import { Client, XmtpError } from "@xmtp/node-sdk";

// Public uint64 fields retain their generated bigint width.
export function checkServerConfiguration(client: Client): void {
  const configuration: ServerConfiguration = client.serverConfiguration;
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
  const retentionSeconds: bigint[] = [
    retention.groupMessageSeconds,
    retention.welcomeSeconds,
    retention.keyPackageSeconds,
  ];

  const limits: LimitsConfiguration = configuration.limits;
  const limitValues: bigint[] = [
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
  ];
  const rateValues: number[] = [
    limits.maxUpdateFramesPerSecond,
    limits.maxUpdateBurst,
    limits.maxPingFramesPerSecond,
    limits.maxPingBurst,
  ];

  const mls: MlsConfiguration = configuration.mls;
  const maxGroupMembers: bigint = mls.maxGroupMembers;
  const maxInstallationsPerInbox: bigint = mls.maxInstallationsPerInbox;
  const commitLogEnabled: boolean | undefined = mls.commitLogEnabled;

  const _values = [
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
    rateValues,
    maxGroupMembers,
    maxInstallationsPerInbox,
    commitLogEnabled,
  ];
}

declare const maxRequestBytes: LimitsConfiguration["maxRequestBytes"];
// @ts-expect-error A generated uint64 is bigint, not number.
export const notANumber: number = maxRequestBytes;

export async function checkFetch(
  url: string,
  backend: Backend,
): Promise<ServerConfiguration[]> {
  return [
    await Client.fetchServerConfiguration({ url }),
    await Client.fetchServerConfiguration({
      url,
      appVersion: "type-test/8.0.0",
    }),
    await Client.fetchServerConfiguration(backend),
  ];
}

export async function checkRefresh(client: Client): Promise<void> {
  const _refreshed: ServerConfiguration =
    await client.refreshServerConfiguration();
}

// Each configuration failure has a distinct public error kind.
export function checkErrors(error: unknown): string | undefined {
  if (error instanceof XmtpError.ConfigurationUnavailable)
    return error.details.code;
  if (error instanceof XmtpError.ConfigurationInvalid)
    return error.details.code;
  if (error instanceof XmtpError.BackendMismatch) return error.details.code;
  if (error instanceof XmtpError.ClientVersionTooOld) return error.details.code;
  if (error instanceof XmtpError.AuthRequired) return error.details.code;
  if (error instanceof XmtpError.ChainNotAccepted) return error.details.code;
  return undefined;
}
