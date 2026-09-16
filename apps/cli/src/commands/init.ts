import { randomBytes } from "node:crypto";
import { access, mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { Command, Errors, Flags } from "@oclif/core";
import { generatePrivateKey } from "viem/accounts";
import { parseBackendUrl, parseEnvironmentLabel } from "../utils/backend.js";
import { DEFAULT_ENV_PATH } from "../utils/config.js";

export default class Init extends Command {
  static description = `Initialize XMTP CLI configuration by generating wallet and encryption keys.

This command creates the necessary cryptographic keys for XMTP:
- A wallet private key (32 bytes, used for identity)
- A database encryption key (32 bytes, used to encrypt local data)

By default, keys are written to ${DEFAULT_ENV_PATH} so they can be read
globally. Use --output to write to a custom path, or --stdout to
print keys to the console instead.`;

  static examples = [
    {
      command:
        "<%= config.bin %> <%= command.id %> --backend-url http://127.0.0.1:5050",
      description: `Generate keys and write to ${DEFAULT_ENV_PATH}`,
    },
    {
      command: "<%= config.bin %> <%= command.id %> --stdout",
      description: "Generate keys and print to console",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --backend-url http://127.0.0.1:5050 --output ./my-project/.env --env local",
      description: "Write to a custom path",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --force",
      description: "Overwrite existing .env file",
    },
  ];

  static flags = {
    stdout: Flags.boolean({
      description: "Output keys to stdout instead of writing to file",
      default: false,
    }),
    output: Flags.string({
      char: "o",
      description: "Path to write .env file",
      helpValue: "<path>",
    }),
    "backend-url": Flags.string({
      description: "XMTP backend URL",
      helpValue: "<url>",
      required: true,
    }),
    env: Flags.string({
      description: "Database label to set in generated file",
      default: "local",
    }),
    force: Flags.boolean({
      char: "f",
      description: "Overwrite existing .env file",
      default: false,
    }),
  };

  catch(error: Error): never {
    if (error instanceof Errors.CLIError) {
      this.logToStderr(error.message);
      this.exit(error.oclif.exit ?? 1);
    }

    const cliError = new Errors.CLIError(error.message);
    (cliError as Error & { showHelp?: boolean }).showHelp = false;
    throw cliError;
  }

  async run(): Promise<void> {
    const { flags } = await this.parse(Init);
    parseBackendUrl(flags["backend-url"]);
    parseEnvironmentLabel(flags.env);

    // Generate keys
    const walletKey = generatePrivateKey();
    const dbEncryptionKey = randomBytes(32).toString("hex");

    const envContent = `XMTP_WALLET_KEY=${walletKey}
XMTP_DB_ENCRYPTION_KEY=${dbEncryptionKey}
XMTP_BACKEND_URL=${flags["backend-url"]}
XMTP_ENV=${flags.env}
`;

    if (flags.stdout) {
      this.log(envContent);
      return;
    }

    const outputPath = resolve(flags.output ?? DEFAULT_ENV_PATH);

    // Ensure parent directory exists
    await mkdir(dirname(outputPath), { recursive: true });

    // Check if file exists
    let fileExists = false;
    try {
      await access(outputPath);
      fileExists = true;
    } catch {
      // File doesn't exist, continue
    }

    if (fileExists && !flags.force) {
      this.error(
        `File already exists: ${outputPath}\nUse --force to overwrite.`,
      );
    }

    await writeFile(outputPath, envContent, "utf-8");
    this.log(`Configuration written to ${outputPath}`);
  }
}
