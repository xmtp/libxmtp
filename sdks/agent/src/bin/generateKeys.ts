#!/usr/bin/env node
import { getRandomValues } from "node:crypto";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { generatePrivateKey } from "viem/accounts";

const generateClientKeys = () => {
  const randomValues = getRandomValues(new Uint8Array(32));
  const dbEncryptionKey = Buffer.from(randomValues).toString("hex");
  return {
    XMTP_DB_ENCRYPTION_KEY: dbEncryptionKey,
    XMTP_WALLET_KEY: generatePrivateKey(),
  };
};

/**
 * Generates client keys and saves them to a .env file in the project root.
 * This script creates the necessary environment variables for XMTP agent initialization.
 *
 * By default it refuses to overwrite an existing .env file. The write itself uses the
 * "wx" flag (create-only), so there is no check-then-write race. Pass --force to
 * overwrite an existing .env on purpose.
 */
function main() {
  try {
    if (!process.env.INIT_CWD) {
      throw new Error(
        `Cannot invoke script because "process.env.INIT_CWD" wasn't found.`,
      );
    }
    const envFilePath = join(process.env.INIT_CWD, ".env");
    const force = process.argv.includes("--force");

    // Generate the client keys
    const keys = generateClientKeys();

    // Create the .env file content
    const envContent =
      Object.entries(keys)
        .map(([key, value]) => `${key}=${value}`)
        .join("\n") + "\n";

    if (force) {
      console.warn(
        `--force passed: any previous ${envFilePath} contents will be overwritten.\n` +
          `   Any previous XMTP_WALLET_KEY, XMTP_DB_ENCRYPTION_KEY, or other\n` +
          `   variables in this file will be permanently lost.`,
      );
    }

    try {
      // "wx" creates the file and fails with EEXIST if it already exists.
      writeFileSync(envFilePath, envContent, {
        encoding: "utf8",
        flag: force ? "w" : "wx",
      });
    } catch (writeError) {
      if (!force && (writeError as NodeJS.ErrnoException).code === "EEXIST") {
        console.error(
          `Refusing to overwrite existing file: ${envFilePath}\n` +
            `   It may already contain XMTP_WALLET_KEY, XMTP_DB_ENCRYPTION_KEY,\n` +
            `   or other variables that would be permanently lost.\n\n` +
            `   Re-run with --force to regenerate credentials and discard\n` +
            `   the current .env contents:\n\n` +
            `       pnpm gen:keys --force\n`,
        );
        process.exit(1);
      }
      throw writeError;
    }

    console.log("✅ Successfully generated client keys and saved to .env file");
    console.log(`📁 File location: ${envFilePath}`);
    console.log("🔑 Generated keys:");
    Object.keys(keys).forEach((key) => {
      console.log(`   - ${key}`);
    });
  } catch (error) {
    console.error("❌ Error generating client keys:", error);
    process.exit(1);
  }
}

main();
