import { Flags } from "@oclif/core";
import { Timestamp } from "@xmtp/node-sdk";
import type { ConversationOrder } from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";
import { isDm, isGroup } from "@/utils/conversation";
import { consentStateMap, conversationTypeMap } from "@/utils/enums";

export default class ConversationsList extends BaseCommand {
  static description = `List conversations, newest first.

Lists one page of conversations for the current client with optional
filtering: 100 by default, at most 1000 with --limit.

Use --type to filter by conversation type (dm or group).
Use --consent-state to filter by consent state (repeatable).
Use --order-by to control sort order (created-at or last-activity).
Use --created-after / --created-before to filter by creation time.
To read the next page in created-at order, pass the last conversation's
createdAtNs as --created-before and its id as --before-id.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %>",
      description: "List the 100 most recently active conversations",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --order-by created-at --created-before <createdAtNs> --before-id <id>",
      description: "List the next page after a conversation, by creation time",
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
      description:
        "Maximum number of conversations to return, from 1 to 1000 (default 100)",
      helpValue: "<number>",
    }),
    "before-id": Flags.string({
      description:
        "Resume after this conversation; needs --created-before in created-at order",
      helpValue: "<id>",
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
      beforeId: flags["before-id"],
    });

    const output = await Promise.all(
      conversations.map(async (conversation) => {
        const snapshot = await conversation.state();
        const state = "common" in snapshot ? snapshot.common : snapshot;
        const base = {
          id: conversation.id,
          type: isGroup(conversation) ? "group" : "dm",
          createdAt: conversation.createdAt.date.toISOString(),
          createdAtNs: conversation.createdAt.ns.toString(),
          consentState: state.consentState,
          isActive: state.isActive,
        };

        if ("common" in snapshot) {
          return {
            ...base,
            name: snapshot.name,
            description: snapshot.description,
            imageUrl: snapshot.imageUrl,
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
