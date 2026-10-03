import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";

import {
  Client,
  initLogging,
  type BackendSource,
  type LogLevel,
  type Signer,
} from "@xmtp/node-sdk";
import { isHex, toBytes, toHex } from "viem";
import { privateKeyToAccount } from "viem/accounts";

import type { XmtpConfig } from "./config.js";

const LOG_LEVELS = {
  off: "off",
  error: "error",
  warn: "warn",
  info: "info",
  debug: "debug",
  trace: "trace",
} as const;

type LogLevelKey = keyof typeof LOG_LEVELS;

function isLogLevelKey(value: string): value is LogLevelKey {
  return value in LOG_LEVELS;
}

function parseLogLevel(value: string | undefined): LogLevel | undefined {
  if (value === undefined) {
    return undefined;
  }
  if (!isLogLevelKey(value)) {
    const validLevels = Object.keys(LOG_LEVELS).join(", ");
    throw new Error(
      `Invalid log level: ${value}. Valid levels: ${validLevels}`,
    );
  }
  return LOG_LEVELS[value];
}

export function createEOASigner(walletKey: string): Signer {
  const hex = walletKey.startsWith("0x") ? walletKey : `0x${walletKey}`;
  if (!isHex(hex, { strict: true })) {
    throw new Error("Invalid wallet key: must be a hex string");
  }
  const account = privateKeyToAccount(hex);
  return {
    identity: () =>
      Promise.resolve({
        kind: "ethereum" as const,
        identifier: account.address.toLowerCase(),
      }),
    kind: () => Promise.resolve({ kind: "eoa" as const }),
    sign: async ({ text }) => ({
      kind: "ecdsa",
      value: toBytes(await account.signMessage({ message: text })),
    }),
  };
}

export function hexToBytes(value: string): Uint8Array {
  const hex = value.startsWith("0x") ? value : `0x${value}`;
  if (!isHex(hex, { strict: true })) {
    throw new Error(`Invalid hex string: ${value}`);
  }
  return toBytes(hex);
}

/** Decode an installation ID and return the public SDK's canonical hex form. */
export function installationIdFromHex(value: string): string {
  const bytes = hexToBytes(value);
  if (bytes.length !== 32) {
    throw new Error("Installation ID must contain exactly 32 bytes");
  }
  return toHex(bytes).slice(2);
}

export async function createClient(
  config: XmtpConfig,
  networkOptions: BackendSource,
): Promise<Client> {
  if (!config.walletKey) {
    throw new Error(
      "Wallet key is required. Set XMTP_WALLET_KEY, use --wallet-key, or run 'init' to generate one.",
    );
  }

  if (!config.dbEncryptionKey) {
    throw new Error(
      "Database encryption key is required. Set XMTP_DB_ENCRYPTION_KEY, use --db-encryption-key, or run 'init' to generate one.",
    );
  }

  const signer = createEOASigner(config.walletKey);

  if (config.dbPath) {
    await mkdir(dirname(config.dbPath), { recursive: true });
  }

  const level = parseLogLevel(config.logLevel);
  if (level) await initLogging({ level, structured: config.structuredLogging });
  const client = await Client.create(signer, {
    backend: networkOptions,
    storage: {
      location: config.dbPath
        ? {
            dbPath: config.dbPath,
            attachmentsDir: `${config.dbPath}.attachments`,
          }
        : "default",
      label: config.env,
      encryptionKey: hexToBytes(config.dbEncryptionKey),
    },
    deviceSync: !config.disableDeviceSync,
  });

  return client;
}
