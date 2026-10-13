import { Args } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { requireGroup } from "@/utils/conversation";
import { identifierKindFlag, publicIdentity } from "@/utils/identity";

export default class ConversationRemoveMembers extends BaseCommand {
  static description = `Remove members from a group conversation.

Removes one or more members from a group conversation using their Ethereum addresses,
or their handles with --identifier-kind handle.
This command is only available for group conversations, not DMs.

Provide identifiers as space-separated arguments to remove multiple members.
The identifiers must belong to existing members of the group.

Requires appropriate permissions to remove members (based on group settings).`;

  static examples = [
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> <address>",
      description: "Remove a single member from the group",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> <address> <address-2>",
      description: "Remove multiple members from the group",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> <address> --json",
      description: "Output as JSON for scripting",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> nick#2341 --identifier-kind handle",
      description: "Remove a member by handle",
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
    const { args, argv, flags } = await this.parse(ConversationRemoveMembers);
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
    await group.removeMembers(identifiers);

    this.output({
      success: true,
      conversationId: args.id,
      removedMembers: addresses,
      count: addresses.length,
    });
  }
}
