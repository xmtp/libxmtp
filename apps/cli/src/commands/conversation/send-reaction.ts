import { Args, Flags } from "@oclif/core";
import type { Reaction } from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";

export default class ConversationSendReaction extends BaseCommand {
  static description = `Send a reaction to a message in a conversation.

Sends a reaction (emoji) to a specific message within a conversation.
The action argument controls whether the reaction is added or removed.

Use --optimistic to send the reaction optimistically (queued locally
and published via 'conversation publish-messages').`;

  static examples = [
    {
      command:
        '<%= config.bin %> <%= command.id %> <conversation-id> <message-id> add "👍"',
      description: "React to a message with thumbs up",
    },
    {
      command:
        '<%= config.bin %> <%= command.id %> <conversation-id> <message-id> remove "👍"',
      description: "Remove a thumbs up reaction",
    },
    {
      command:
        '<%= config.bin %> <%= command.id %> <conversation-id> <message-id> add "🔥" --optimistic',
      description: "Send reaction optimistically",
    },
  ];

  static args = {
    id: Args.string({
      description: "The conversation ID",
      required: true,
    }),
    messageId: Args.string({
      description: "The message ID to react to",
      required: true,
    }),
    action: Args.string({
      description: "Reaction action",
      required: true,
      options: ["add", "remove"],
    }),
    content: Args.string({
      description: "The reaction content (e.g., emoji)",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
    optimistic: Flags.boolean({
      description:
        "Send optimistically (queued locally and published via 'conversation publish-messages')",
      default: false,
    }),
  };

  async run(): Promise<void> {
    const { args, flags } = await this.parse(ConversationSendReaction);
    const client = await this.initClient();

    const conversation = await client.conversations.getById(args.id);

    if (!conversation) {
      this.error(`Conversation not found: ${args.id}`);
    }

    const message = await client.conversations.getMessageById(args.messageId);

    if (!message) {
      this.error(`Message not found: ${args.messageId}`);
    }

    const reaction: Reaction = {
      content: args.content,
      action: args.action === "add" ? "added" : "removed",
      schema: "unicode",
    };

    const messageId = await conversation.sendReaction(
      args.messageId,
      message.senderInboxId,
      reaction,
      {
        optimistic: flags.optimistic,
        shouldPush: false,
      },
    );

    this.output({
      success: true,
      messageId,
      conversationId: args.id,
      optimistic: flags.optimistic,
      shouldPush: false,
      reaction: {
        reference: args.messageId,
        content: args.content,
        action: args.action,
      },
    });
  }
}
