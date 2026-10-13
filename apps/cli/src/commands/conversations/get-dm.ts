import { Args, Flags } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { conversationState } from "@/utils/conversation";
import { handleNotFoundMessage } from "@/utils/handles";
import {
  IDENTIFIER_KINDS,
  publicIdentity,
  unresolvedHandles,
} from "@/utils/identity";
import { memberDetails } from "@/utils/members";

export default class ConversationsGetDm extends BaseCommand {
  static description = `Get a DM conversation by address, handle, or inbox ID.

Looks up a direct message conversation using an Ethereum address, a
handle, or an inbox ID.

When an address is provided (starts with 0x), the network is queried to
resolve the identifier and find the DM. When an inbox ID is provided,
the local cache is searched directly. With --identifier-kind, the argument
is an identifier of that kind: use --identifier-kind handle for a handle
such as nick#2341. An old handle of an inbox also finds it.`;

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
      command:
        "<%= config.bin %> <%= command.id %> nick#2341 --identifier-kind handle",
      description: "Get DM by handle",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <address> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static args = {
    addressOrInboxId: Args.string({
      description: "Ethereum address (0x...), handle, or inbox ID",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
    "identifier-kind": Flags.option({
      options: IDENTIFIER_KINDS,
      description:
        "Read the argument as this type of identifier instead of detecting an address or an inbox ID",
    })(),
  };

  async run(): Promise<void> {
    const { args, flags } = await this.parse(ConversationsGetDm);
    const client = await this.initClient();

    const kind =
      flags["identifier-kind"] ??
      (args.addressOrInboxId.startsWith("0x") ? "ethereum" : undefined);

    const identity = kind
      ? publicIdentity(args.addressOrInboxId, kind)
      : undefined;
    const dm = identity
      ? await client.conversations.getDmByIdentity(identity)
      : await client.conversations.getDmByInboxId(args.addressOrInboxId);

    if (!dm) {
      // A handle that no inbox holds finds no DM, with no error. Say so.
      const missing = identity
        ? await unresolvedHandles(client, [identity])
        : [];
      if (missing.length > 0) {
        this.error(handleNotFoundMessage(missing));
      }
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
      members: await memberDetails(client, members, this.jsonOutput),
    });
  }
}
