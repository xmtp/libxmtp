import { Args } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { conversationState } from "@/utils/conversation";
import { memberDetails } from "@/utils/members";

export default class ConversationsGetDm extends BaseCommand {
  static description = `Get a DM conversation by address or inbox ID.

Looks up a direct message conversation using either an Ethereum address
or an inbox ID.

When an address is provided (starts with 0x), the network is queried to
resolve the identifier and find the DM. When an inbox ID is provided,
the local cache is searched directly.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> <address>",
      description: "Get DM by Ethereum address",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <inbox-id>",
      description: "Get DM by inbox ID",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <address> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static args = {
    addressOrInboxId: Args.string({
      description: "Ethereum address (0x...) or inbox ID",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
  };

  async run(): Promise<void> {
    const { args } = await this.parse(ConversationsGetDm);
    const client = await this.initClient();

    const isAddress = args.addressOrInboxId.startsWith("0x");

    const dm = isAddress
      ? await client.conversations.getDmByIdentity({
          identifier: args.addressOrInboxId.toLowerCase(),
          kind: "ethereum" as const,
        })
      : await client.conversations.getDmByInboxId(args.addressOrInboxId);

    if (!dm) {
      this.error(`DM not found for: ${args.addressOrInboxId}`);
    }

    const state = await conversationState(dm);
    const members = await dm.members();

    this.output({
      id: dm.id,
      peerInboxId: await dm.peerInboxId(),
      createdAt: dm.createdAt.date.toISOString(),
      consentState: state.consentState,
      isActive: state.isActive,
      addedByInboxId: dm.addedByInboxId,
      creatorInboxId: dm.creatorInboxId,
      members: await memberDetails(client, members),
    });
  }
}
