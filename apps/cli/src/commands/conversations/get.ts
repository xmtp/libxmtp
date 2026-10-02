import { Args } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { conversationState } from "@/utils/conversation";
import { isDm, isGroup } from "@/utils/conversation";

export default class ConversationsGet extends BaseCommand {
  static description = `Get a conversation by ID.

Retrieves detailed information about a specific conversation (group or DM)
using its unique identifier.

The output includes:
- Conversation ID
- Type (group or dm)
- Created timestamp
- Consent state
- Active status
- Members list
- Group-specific: name, description, image URL, admins, permissions
- DM-specific: peer inbox ID

Use this to inspect the full details of a specific conversation.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> <conversation-id>",
      description: "Get conversation by ID",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <conversation-id> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static args = {
    id: Args.string({
      description: "The conversation ID to retrieve",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
  };

  async run(): Promise<void> {
    const { args } = await this.parse(ConversationsGet);
    const client = await this.initClient();

    const conversation = await client.conversations.getById(args.id);

    if (!conversation) {
      this.error(`Conversation not found: ${args.id}`);
    }

    const state = await conversationState(conversation);
    const members = await conversation.members();

    const base = {
      id: conversation.id,
      type: isGroup(conversation) ? "group" : "dm",
      createdAt: conversation.createdAt.date.toISOString(),
      createdAtNs: conversation.createdAt.ns,
      consentState: state.consentState,
      isActive: state.isActive,
      addedByInboxId: conversation.addedByInboxId,
      creatorInboxId: conversation.creatorInboxId,
      memberCount: members.length,
      members: members.map((m) => ({
        inboxId: m.inboxId,
        accountIdentifiers: m.identities,

        permissionLevel: m.permissionLevel,
        consentState: m.consentState,
      })),
    };

    if (isGroup(conversation)) {
      const groupState = await conversation.state();
      const permissions = groupState.permissions;
      const admins = await conversation.listAdmins();
      const superAdmins = await conversation.listSuperAdmins();

      this.output({
        ...base,
        name: (await conversation.state()).name,
        description: (await conversation.state()).description,
        imageUrl: (await conversation.state()).imageUrl,
        admins,
        superAdmins,
        permissions: {
          policyType: permissions.policyType,
          policySet: permissions.policySet,
        },
      });
    } else if (isDm(conversation)) {
      this.output({
        ...base,
        peerInboxId: await conversation.peerInboxId(),
      });
    } else {
      this.output(base);
    }
  }
}
