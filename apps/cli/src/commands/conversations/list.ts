import { Flags } from "@oclif/core";
import { Timestamp } from "@xmtp/node-sdk";
import type { ConversationOrder } from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";
import { conversationState } from "@/utils/conversation";
import { isDm, isGroup } from "@/utils/conversation";
import { consentStateMap, conversationTypeMap } from "@/utils/enums";

export default class ConversationsList extends BaseCommand {
  static description = `List all conversations.

Lists conversations for the current client with optional filtering.

Use --type to filter by conversation type (dm or group).
Use --consent-state to filter by consent state (repeatable).
Use --order-by to control sort order (created-at or last-activity).
Use --created-after / --created-before to filter by creation time.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %>",
      description: "List all conversations",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --type dm",
      description: "List only DMs",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --consent-state allowed",
      description: "List only allowed conversations",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --consent-state allowed --consent-state unknown",
      description: "List allowed and unknown conversations",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --order-by last-activity --limit 10",
      description: "List 10 most recently active conversations",
    },
  ];

  static flags = {
    ...BaseCommand.baseFlags,
    sync: Flags.boolean({
      description: "Sync conversations from network before listing",
      default: false,
    }),
    type: Flags.option({
      options: ["dm", "group"] as const,
      description: "Filter by conversation type",
    })(),
    limit: Flags.integer({
      description: "Maximum number of conversations to return",
      helpValue: "<number>",
    }),
    "consent-state": Flags.option({
      options: ["allowed", "denied", "unknown"] as const,
      description: "Filter by consent state (repeatable)",
      multiple: true,
    })(),
    "order-by": Flags.option({
      options: ["created-at", "last-activity"] as const,
      description: "Sort order for results",
    })(),
    "created-after": Flags.string({
      description:
        "Only include conversations created after this timestamp (nanoseconds)",
      helpValue: "<ns>",
    }),
    "created-before": Flags.string({
      description:
        "Only include conversations created before this timestamp (nanoseconds)",
      helpValue: "<ns>",
    }),
  };

  async run(): Promise<void> {
    const { flags } = await this.parse(ConversationsList);
    const client = await this.initClient();

    if (flags.sync) {
      await client.conversations.sync();
    }

    const orderByMap: Record<string, ConversationOrder> = {
      "created-at": "createdAt",
      "last-activity": "lastActivity",
    };

    const conversations = await client.conversations.list({
      limit: flags.limit,
      consentStates: flags["consent-state"]?.map((s) => consentStateMap[s]),
      kind: flags.type ? conversationTypeMap[flags.type] : undefined,
      orderBy: flags["order-by"] ? orderByMap[flags["order-by"]] : undefined,
      createdAfter: flags["created-after"]
        ? new Timestamp(
            this.parseBigInt(flags["created-after"], "created-after")!,
          )
        : undefined,
      createdBefore: flags["created-before"]
        ? new Timestamp(
            this.parseBigInt(flags["created-before"], "created-before")!,
          )
        : undefined,
    });

    const output = await Promise.all(
      conversations.map(async (conversation) => {
        const state = await conversationState(conversation);
        const base = {
          id: conversation.id,
          type: isGroup(conversation) ? "group" : "dm",
          createdAt: conversation.createdAt.date.toISOString(),
          consentState: state.consentState,
          isActive: state.isActive,
        };

        if (isGroup(conversation)) {
          return {
            ...base,
            name: (await conversation.state()).name,
            description: (await conversation.state()).description,
            imageUrl: (await conversation.state()).imageUrl,
          };
        } else if (isDm(conversation)) {
          return {
            ...base,
            peerInboxId: await conversation.peerInboxId(),
          };
        }

        return base;
      }),
    );

    this.output(output);
  }
}
