import { type Client, type LogLevel } from "@xmtp/node-sdk";

import type { Agent } from "@/core/Agent";

const validLogLevels: LogLevel[] = [
  "off",
  "error",
  "warn",
  "info",
  "debug",
  "trace",
];

const isLogLevel = (level: string): level is LogLevel => {
  return validLogLevels.includes(level as LogLevel);
};

/** Return the log levels accepted by the Agent SDK. */
export const getValidLogLevels = (): LogLevel[] => {
  return [...validLogLevels];
};

/** Parse a case-insensitive log level, returning `null` for invalid input. */
export const parseLogLevel = (rawLevel: string) => {
  const normalizedLevel = rawLevel.toLowerCase();

  if (isLogLevel(normalizedLevel)) {
    return normalizedLevel;
  }

  return null;
};

/** Log client, installation, and key-package details for operational debugging. */
export const logDetails = async <ContentTypes>(agent: Agent<ContentTypes>) => {
  const xmtp = `\x1b[38;2;252;76;52m
    ██╗  ██╗███╗   ███╗████████╗██████╗
    ╚██╗██╔╝████╗ ████║╚══██╔══╝██╔══██╗
     ╚███╔╝ ██╔████╔██║   ██║   ██████╔╝
     ██╔██╗ ██║╚██╔╝██║   ██║   ██╔═══╝
    ██╔╝ ██╗██║ ╚═╝ ██║   ██║   ██║
    ╚═╝  ╚═╝╚═╝     ╚═╝   ╚═╝   ╚═╝
  \x1b[0m`;

  const client = agent.client;
  const clientsByAddress = client.identity.identifier;
  const inboxId = client.inboxId;
  const installationId = client.installationId;

  const urls = [`http://xmtp.chat/dm/${clientsByAddress}`];

  const conversations = await client.conversations.list();
  const inboxState = await client.inboxState(false);
  const keyPackageStatuses = await client.keyPackageStatuses([installationId]);

  let createdDate = new Date();
  let expiryDate = new Date();

  // Extract key package status for the specific installation
  const keyPackageStatus = keyPackageStatuses.get(installationId) ?? {};
  if (keyPackageStatus.lifetime) {
    createdDate = new Date(Number(keyPackageStatus.lifetime.notBefore) * 1000);
    expiryDate = new Date(Number(keyPackageStatus.lifetime.notAfter) * 1000);
  }
  console.log(`
    ${xmtp}

    ✓ XMTP Client:
    • InboxId: ${inboxId}
    • LibXMTP Version: ${agent.libxmtpVersion}
    • Address: ${clientsByAddress}
    • Conversations: ${conversations.length}
    • Installations: ${inboxState.installations.length}
    • InstallationId: ${installationId}
    • Key Package created: ${createdDate.toLocaleString()}
    • Key Package valid until: ${expiryDate.toLocaleString()}
    ${urls.map((url) => `• URL: ${url}`).join("\n")}`);
};

/**
 * Returns a URL to test your agent on https://xmtp.chat/ (for development purposes only).
 *
 * @param client - Your XMTP client
 * @returns The URL to test your agent with
 */
export const getTestUrl = <_ContentTypes>(client: Client) => {
  const address = client.identity.identifier;
  return `http://xmtp.chat/dm/${address}`;
};

/** Registration details for the client's inbox and installation. */
export type InstallationInfo = {
  /** Number of registered installations in the inbox. */
  totalInstallations: number;
  /** This client's installation ID. */
  installationId: string;
  /** ID of the newest registered installation, or null if none is found. */
  mostRecentInstallationId: null | string;
  /** Whether this client is the newest registered installation. */
  isMostRecent: boolean;
};

/** Read installation count and determine whether this client is newest. */
export const getInstallationInfo = async <_ContentTypes>(
  client: Client,
): Promise<InstallationInfo> => {
  const myInboxId = client.inboxId;
  const myInstallationId = client.installationId;

  const inboxStates = await client.inboxStates([myInboxId], true);

  const installations =
    inboxStates.find((state) => state.inboxId === myInboxId)?.installations ||
    [];

  const sortedInstallations = [...installations].sort((a, b) => {
    const aTime = a.createdAt?.ns ?? 0n;
    const bTime = b.createdAt?.ns ?? 0n;
    return bTime > aTime ? 1 : bTime < aTime ? -1 : 0;
  });

  const mostRecentInstallation = sortedInstallations[0];

  const info: InstallationInfo = {
    totalInstallations: installations.length,
    installationId: myInstallationId,
    mostRecentInstallationId: null,
    isMostRecent: false,
  };

  if (mostRecentInstallation) {
    const mostRecentIdHex = mostRecentInstallation.id;
    info.mostRecentInstallationId = mostRecentIdHex;
    info.isMostRecent = myInstallationId === mostRecentIdHex;
  }

  return info;
};
