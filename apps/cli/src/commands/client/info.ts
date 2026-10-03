import { BaseCommand } from "@/baseCommand";
import { formatSections } from "@/utils/output";

export default class ClientInfo extends BaseCommand {
  static description = `Display information about the current XMTP client.

Shows the client's identity information including:
- Wallet address (derived from private key)
- Inbox ID (XMTP network identifier)
- Installation ID (unique to this device/instance)
- Registration status
- SDK version information

This command is useful for debugging and verifying your XMTP configuration.
The inbox ID is your unique identifier on the XMTP network and is derived
from your wallet address. The installation ID is unique to each device or
application instance using your identity.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %>",
      description: "Display client info using .env configuration",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static flags = {
    ...BaseCommand.baseFlags,
  };

  async run(): Promise<void> {
    const client = await this.initClient();

    const options = client.options;
    const properties = {
      address: client.identity.identifier,
      inboxId: client.inboxId,
      installationId: client.installationId,
      isRegistered: await client.isRegistered(),
      appVersion: client.appVersion,
      libxmtpVersion: client.libxmtpVersion,
    };

    const clientOptions = {
      env: this.getConfig().env ?? "local",
      backendUrl: this.getConfig().backendUrl,
      storage: options.storage,
      dbPath: await client.storage.path(),
      deviceSync: options.deviceSync,
      registration: options.registration,
      appVersion: client.appVersion,
    };

    if (this.jsonOutput) {
      this.output({ properties, options: clientOptions });
    } else {
      this.log(
        formatSections(
          [
            { title: "Client", data: properties },
            { title: "Options", data: clientOptions },
          ],
          2,
        ),
      );
    }
  }
}
