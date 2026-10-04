import { Flags } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";

export default class PreferencesStream extends BaseCommand {
  static description = `Stream all preference changes.

Listens for all user preference updates in real-time. This includes:
- Consent state changes (ConsentUpdate)
- HMAC key updates (HmacKeyUpdate)
- Lagged events with the number of discarded updates (resync after these)

Each update batch is output as it arrives.

The stream will continue until:
- The timeout is reached (if --timeout is specified)
- The count limit is reached (if --count is specified)
- The process is interrupted (Ctrl+C)

This is useful for:
- Full preference synchronization across devices
- Monitoring all preference changes
- Debugging preference-related issues

The stream starts without a separate preferences sync. Call preferences sync
first if you need a current snapshot before listening for updates.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %>",
      description: "Stream all preference changes indefinitely",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --timeout 60",
      description: "Stream for 60 seconds",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --count 5",
      description: "Stream until 5 preference update batches received",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --timeout 120 --count 10",
      description: "Stream for up to 120 seconds or 10 updates",
    },
    {
      command: "<%= config.bin %> <%= command.id %> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static flags = {
    ...BaseCommand.baseFlags,
    timeout: Flags.integer({
      description: "Stop streaming after N seconds",
      helpValue: "<seconds>",
    }),
    count: Flags.integer({
      description: "Stop after receiving N preference update batches",
      helpValue: "<number>",
    }),
  };

  async run(): Promise<void> {
    const { flags } = await this.parse(PreferencesStream);
    const client = await this.initClient();

    let updateCount = 0;
    const maxCount = flags.count;
    const timeoutMs = flags.timeout ? flags.timeout * 1000 : undefined;

    const stream = await client.events({
      kinds: ["consent.changed", "hmac_keys.updated"],
      referencesOwnMessages: false,
    });

    // Set up timeout if specified
    let timeoutId: NodeJS.Timeout | undefined;
    if (timeoutMs) {
      timeoutId = setTimeout(() => {
        void stream.return();
      }, timeoutMs);
    }

    const onSigint = () => {
      void stream.return();
    };
    process.once("SIGINT", onSigint);

    try {
      for await (const event of stream) {
        if (event.kind === "lagged") {
          this.streamOutput({
            timestamp: new Date().toISOString(),
            warning: { type: "Lagged", discarded: event.lagged.discarded },
          });
          continue;
        }
        let update;
        if (event.kind === "consent.changed") {
          update = {
            type: "ConsentUpdate",
            entityType:
              event.consent_changed.entityKind === "inbox"
                ? "inbox_id"
                : "conversation_id",
            entity: event.consent_changed.entity,
            state: event.consent_changed.state,
          };
        } else if (event.kind === "hmac_keys.updated") {
          update = {
            type: "HmacKeyUpdate",
            keys: await client.conversations.hmacKeys(),
          };
        } else {
          throw new Error(`Unexpected preference event: ${event.kind}`);
        }
        this.streamOutput({
          timestamp: new Date().toISOString(),
          updates: [update],
        });

        updateCount++;
        if (maxCount && updateCount >= maxCount) {
          break;
        }
      }
    } finally {
      process.off("SIGINT", onSigint);
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      await stream.return();
    }
  }
}
