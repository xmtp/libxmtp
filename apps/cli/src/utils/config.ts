import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { cwd, env, loadEnvFile } from "node:process";
import { defaultDbPath } from "./backend.js";

export const DEFAULT_HOME_DIR = join(homedir(), ".xmtp");
export const DEFAULT_ENV_PATH = join(DEFAULT_HOME_DIR, ".env");
export const VALID_LOG_LEVELS = [
  "off",
  "error",
  "warn",
  "info",
  "debug",
  "trace",
] as const;

export type XmtpConfig = {
  walletKey?: string;
  dbEncryptionKey?: string;
  dbPath?: string;
  backendUrl?: string;
  env?: string;
  logLevel?: (typeof VALID_LOG_LEVELS)[number];
  structuredLogging?: boolean;
  disableDeviceSync?: boolean;
  appVersion?: string;
};

function parseLogLevel(value: string | undefined): XmtpConfig["logLevel"] {
  return VALID_LOG_LEVELS.includes(value as (typeof VALID_LOG_LEVELS)[number])
    ? (value as XmtpConfig["logLevel"])
    : undefined;
}

export function loadConfig(envFile?: string): XmtpConfig {
  // Load .env file using Node's built-in mechanism (Node 20.12+)
  // Priority: explicit --env-file > .env in cwd > ~/.xmtp/.env
  if (envFile) {
    try {
      loadEnvFile(resolve(envFile));
    } catch (error) {
      throw new Error(`Failed to load env file: ${envFile}`, { cause: error });
    }
  } else {
    try {
      loadEnvFile(resolve(cwd(), ".env"));
    } catch {
      try {
        loadEnvFile(DEFAULT_ENV_PATH);
      } catch {
        // Silently ignore if neither file exists
      }
    }
  }

  return {
    walletKey: env.XMTP_WALLET_KEY,
    dbEncryptionKey: env.XMTP_DB_ENCRYPTION_KEY,
    dbPath: env.XMTP_DB_PATH,
    backendUrl: env.XMTP_BACKEND_URL,
    env: env.XMTP_ENV,
    logLevel: parseLogLevel(env.XMTP_LOG_LEVEL),
    structuredLogging:
      env.XMTP_STRUCTURED_LOGGING === "true" ? true : undefined,
    disableDeviceSync:
      env.XMTP_DISABLE_DEVICE_SYNC === "true" ? true : undefined,
    appVersion: env.XMTP_APP_VERSION,
  };
}

export function mergeConfig(
  fileConfig: XmtpConfig,
  flags: Partial<XmtpConfig>,
  defaults?: Partial<XmtpConfig>,
): XmtpConfig {
  const backendUrl = flags.backendUrl ?? fileConfig.backendUrl;
  return {
    walletKey: flags.walletKey ?? fileConfig.walletKey,
    dbEncryptionKey: flags.dbEncryptionKey ?? fileConfig.dbEncryptionKey,
    dbPath:
      flags.dbPath ??
      fileConfig.dbPath ??
      (backendUrl ? defaultDbPath(backendUrl) : undefined),
    backendUrl,
    env: flags.env ?? fileConfig.env ?? defaults?.env,
    logLevel: flags.logLevel ?? fileConfig.logLevel,
    structuredLogging: flags.structuredLogging ?? fileConfig.structuredLogging,
    disableDeviceSync: flags.disableDeviceSync ?? fileConfig.disableDeviceSync,
    appVersion:
      flags.appVersion ?? fileConfig.appVersion ?? defaults?.appVersion,
  };
}
