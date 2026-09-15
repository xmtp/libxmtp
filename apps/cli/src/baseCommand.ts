import { env } from "node:process";
import { createInterface } from "node:readline";
import { Command, Errors, Flags } from "@oclif/core";
import type { Client, NetworkOptions } from "@xmtp/node-sdk";
import { createClient } from "./utils/client.js";
import { parseBackendUrl } from "./utils/backend.js";
import { loadConfig, mergeConfig, type XmtpConfig } from "./utils/config.js";
import { formatHuman, isTTY, jsonStringify } from "./utils/output.js";

export class BaseCommand extends Command {
  /** Flags shared by all commands (including those that don't need a client). */
  static commonFlags = {
    "env-file": Flags.string({
      description: "Path to .env file",
      helpValue: "<path>",
    }),
    "backend-url": Flags.string({
      description: "XMTP backend URL",
      helpValue: "<url>",
    }),
    env: Flags.string({
      description: "Database label",
      helpValue: "<label>",
    }),
    json: Flags.boolean({
      description: "Format output as JSON",
    }),
    verbose: Flags.boolean({
      description: "Show additional diagnostic information",
      default: false,
    }),
  };

  /** Full flag set for commands that create a client. */
  static baseFlags = {
    ...BaseCommand.commonFlags,
    "wallet-key": Flags.string({
      description: "Wallet private key (overrides env)",
      helpValue: "<key>",
    }),
    "db-encryption-key": Flags.string({
      description: "Database encryption key (overrides env)",
      helpValue: "<key>",
    }),
    "db-path": Flags.string({
      description: "Database file path (default: derived from backend URL)",
      helpValue: "<path>",
    }),
    "log-level": Flags.option({
      options: ["off", "error", "warn", "info", "debug", "trace"] as const,
      description: "Logging level",
    })(),
    "structured-logging": Flags.boolean({
      description: "Enable structured JSON logging",
    }),
    "disable-device-sync": Flags.boolean({
      description: "Disable device sync",
    }),
    "app-version": Flags.string({
      description: "App version string",
      helpValue: "<version>",
    }),
  };

  #config: XmtpConfig = {};
  #client?: Client;
  jsonOutput = false;
  verbose = false;

  async init(): Promise<void> {
    await super.init();

    const { flags } = await this.parse(this.constructor as typeof BaseCommand);

    // Load config from .env file
    const fileConfig = loadConfig(flags["env-file"]);

    // Merge with CLI flags (CLI flags take precedence)
    this.#config = mergeConfig(
      fileConfig,
      {
        walletKey: flags["wallet-key"],
        dbEncryptionKey: flags["db-encryption-key"],
        dbPath: flags["db-path"],
        backendUrl: flags["backend-url"],
        env: flags.env,
        logLevel: flags["log-level"],
        structuredLogging: flags["structured-logging"],
        disableDeviceSync: flags["disable-device-sync"],
        appVersion: flags["app-version"],
      },
      // defaults
      {
        appVersion: `xmtp-cli/${this.config.version}`,
        env: "local",
      },
    );

    this.jsonOutput = flags.json || env.XMTP_JSON_OUTPUT === "true";
    this.verbose = flags.verbose || env.XMTP_VERBOSE === "true";
  }

  output(data: unknown): void {
    if (this.jsonOutput) {
      this.log(jsonStringify(data, true));
    } else {
      this.log(formatHuman(data));
    }
  }

  /**
   * Output for streaming commands - uses compact JSONL format (one JSON object per line)
   * for easier parsing when multiple items are streamed.
   */
  streamOutput(data: unknown): void {
    if (this.jsonOutput) {
      this.log(jsonStringify(data));
    } else {
      this.log(formatHuman(data));
    }
  }

  parseBigInt(value: string | undefined, flagName: string): bigint | undefined {
    if (value === undefined) {
      return undefined;
    }
    try {
      return BigInt(value);
    } catch {
      this.error(`Invalid value for --${flagName}: must be a numeric string`);
    }
  }

  async confirmAction(message: string, force: boolean): Promise<void> {
    if (force) {
      return;
    }

    if (!isTTY()) {
      this.error(
        "Cannot confirm in non-interactive terminal. Use --force to skip confirmation.",
      );
    }

    const rl = createInterface({
      input: process.stdin,
      output: process.stderr,
    });

    const answer = await new Promise<string>((resolve) => {
      rl.question(`WARNING: ${message}\nAre you sure? (y/N) `, resolve);
    });

    rl.close();

    if (answer.toLowerCase() !== "y") {
      this.error("Operation cancelled");
    }
  }

  getConfig(): XmtpConfig {
    return this.#config;
  }

  networkOptions(): NetworkOptions {
    const config = this.getConfig();
    if (!config.backendUrl) {
      this.error(
        "Backend URL is required. Set XMTP_BACKEND_URL, use --backend-url, or run 'xmtp init --backend-url <url>'.",
      );
    }
    parseBackendUrl(config.backendUrl);

    const label = config.env ?? "local";
    if (
      label.length === 0 ||
      label === "." ||
      label === ".." ||
      label.includes("/") ||
      label.includes("\\")
    ) {
      this.error(
        'Environment label must be non-empty, must not be "." or "..", and must not contain "/" or "\\".',
      );
    }

    return {
      backendUrl: config.backendUrl,
      env: label,
      appVersion: config.appVersion,
    };
  }

  async initClient(): Promise<Client> {
    const config = this.getConfig();
    const client = await createClient(config, this.networkOptions());
    this.#client = client;

    if (this.verbose) {
      const lines: [string, string][] = [
        ["command", this.id ?? "unknown"],
        ["environment", config.env ?? "local"],
        ["backendUrl", config.backendUrl ?? "unknown"],
        ["dbPath", config.dbPath ?? "in-memory"],
      ];

      if (client.accountIdentifier) {
        lines.push(["address", client.accountIdentifier.identifier]);
      }

      lines.push(
        ["inboxId", client.inboxId],
        ["installationId", client.installationId],
      );

      if (client.libxmtpVersion) {
        lines.push(["libxmtpVersion", client.libxmtpVersion]);
      }

      const maxKeyLen = Math.max(...lines.map(([k]) => k.length));
      const log = this.jsonOutput
        ? (msg: string) => {
            this.logToStderr(msg);
          }
        : (msg: string) => {
            this.log(msg);
          };
      for (const [key, value] of lines) {
        log(`${key.padEnd(maxKeyLen)}  ${value}`);
      }
    }

    return client;
  }

  async run(): Promise<void> {
    // Override in subclasses
  }

  async finally(_error: Error | undefined): Promise<void> {
    if (!this.#client) {
      return;
    }
    try {
      await this.#client.close();
    } catch (error) {
      if (this.verbose) {
        const message = error instanceof Error ? error.message : String(error);
        this.logToStderr(`Failed to close client: ${message}`);
      }
    }
  }

  catch(error: Error): never {
    if (error instanceof Errors.CLIError) {
      this.logToStderr(error.message);
      this.exit(error.oclif.exit ?? 1);
    }

    // Wrap non-CLI errors (e.g., SDK errors) with showHelp for
    // helpful command usage display alongside the error message
    const code = (error as Error & { code?: string }).code;
    const message =
      code === "StorageError::PreTransitionDatabase" && this.#config.dbPath
        ? `${error.message}\nDatabase path: ${this.#config.dbPath}`
        : error.message;
    const cliError = new Errors.CLIError(message);
    const errorWithHelp = cliError as Error & {
      showHelp?: boolean;
      parse?: { input: { argv: string[] } };
    };
    errorWithHelp.showHelp = true;
    errorWithHelp.parse = { input: { argv: this.argv } };
    throw errorWithHelp;
  }
}
