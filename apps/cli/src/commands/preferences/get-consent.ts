import { Flags } from "@oclif/core";
import type { ConsentEntityKind, ConsentState } from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";

export default class PreferencesGetConsent extends BaseCommand {
  static description = `Get consent state for an entity.

Retrieves the current consent state for a specific entity (inbox ID or
conversation/group ID).

Entity types:
- inbox_id: An XMTP inbox identifier
- conversation_id: A conversation or group identifier (same as group_id)

Returns one of:
- allowed: Messages from this entity are welcome
- denied: Messages from this entity are blocked/filtered
- unknown: No consent decision has been made

Use 'preferences sync' first to ensure you have the latest consent data.`;

  static examples = [
    {
      command:
        "<%= config.bin %> <%= command.id %> --entity-type inbox_id --entity <inbox-id>",
      description: "Get consent state for an inbox",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --entity-type conversation_id --entity <conversation-id>",
      description: "Get consent state for a conversation",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --entity-type inbox_id --entity <inbox-id> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static flags = {
    ...BaseCommand.baseFlags,
    "entity-type": Flags.option({
      options: ["inbox_id", "conversation_id"] as const,
      description: "Type of entity to get consent for",
      required: true,
    })(),
    entity: Flags.string({
      description: "The entity identifier (inbox ID or conversation ID)",
      required: true,
    }),
  };

  async run(): Promise<void> {
    const { flags } = await this.parse(PreferencesGetConsent);
    const client = await this.initClient();

    const entityTypeMap: Record<string, ConsentEntityKind> = {
      inbox_id: "inbox",
      conversation_id: "conversation",
    };

    const consentStateNames: Record<ConsentState, string> = {
      ["allowed"]: "allowed",
      ["denied"]: "denied",
      ["unknown"]: "unknown",
    };

    const entityType = entityTypeMap[flags["entity-type"]];

    const state = await client.preferences.consentState(
      entityType === "inbox"
        ? { kind: "inbox", inboxId: flags.entity }
        : { kind: "conversation", conversationId: flags.entity },
    );

    this.output({
      entityType: flags["entity-type"],
      entity: flags.entity,
      state: consentStateNames[state],
    });
  }
}
