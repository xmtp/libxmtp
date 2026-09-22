import { Args } from "@oclif/core";
import { Client } from "@xmtp/node-sdk";
import { BaseCommand } from "@/baseCommand";

export default class AddressAuthorized extends BaseCommand {
  static description = `Check if a wallet address is authorized for an inbox.

Queries the XMTP network to determine if the specified wallet address
is authorized to act on behalf of the given inbox ID. An authorized
address can send and receive messages for that inbox.

This is useful for:
- Verifying that an address has access to a specific inbox
- Debugging authorization issues
- Validating multi-wallet inbox configurations`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> <inbox-id> <address>",
      description: "Check if address is authorized for inbox",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <inbox-id> <address> --json",
      description: "Output as JSON for scripting",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <inbox-id> <address> --backend-url https://backend.example.com",
      description: "Use a custom backend URL",
    },
  ];

  static args = {
    inboxId: Args.string({
      description: "The inbox ID to check authorization for",
      required: true,
    }),
    address: Args.string({
      description: "The wallet address to check (Ethereum address)",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.commonFlags,
  };

  async run(): Promise<void> {
    const { args } = await this.parse(AddressAuthorized);

    const isAuthorized = await Client.isAddressAuthorized(
      args.inboxId,
      args.address.toLowerCase(),
      await this.networkOptions(),
    );

    this.output({
      inboxId: args.inboxId,
      address: args.address,
      isAuthorized,
    });
  }
}
