import { Args } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { requireGroup } from "@/utils/conversation";
import { identifierKindFlag, publicIdentity } from "@/utils/identity";

export default class ConversationAddMembers extends BaseCommand {
  static description = `Add members to a group conversation.

Adds one or more members to a group conversation using their Ethereum addresses,
or their handles with --identifier-kind handle.
This command is only available for group conversations, not DMs.

Provide identifiers as space-separated arguments to add multiple members.

Requires appropriate permissions to add members (based on group settings).`;

  static examples = [
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> <address>",
      description: "Add a single member to the group",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> <address> <address-2>",
      description: "Add multiple members to the group",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> <address> --json",
      description: "Output as JSON for scripting",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> nick#2341 --identifier-kind handle",
      description: "Add a member by handle",
    },
  ];

  static strict = false;

  static args = {
    id: Args.string({
      description: "The conversation ID",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
    "identifier-kind": identifierKindFlag(),
  };

  async run(): Promise<void> {
    const { args, argv, flags } = await this.parse(ConversationAddMembers);
    const client = await this.initClient();

    // Get identifiers from remaining arguments (excluding the conversation ID)
    const addresses = (argv as string[]).filter((a) => a !== args.id);

    if (addresses.length === 0) {
      this.error("At least one identifier is required");
    }

    const conversation = await client.conversations.getById(args.id);

    if (!conversation) {
      this.error(`Conversation not found: ${args.id}`);
    }

    const identifiers = addresses.map((address) =>
      publicIdentity(address, flags["identifier-kind"]),
    );

    const group = requireGroup(conversation);
    await group.addMembers(identifiers);

    this.output({
      success: true,
      conversationId: args.id,
      addedMembers: addresses,
      count: addresses.length,
    });
  }
}
