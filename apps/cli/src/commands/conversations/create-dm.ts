import { Args } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { conversationState } from "@/utils/conversation";
import {
  identifierKindFlag,
  publicIdentity,
  reportUnresolvedHandles,
} from "@/utils/identity";
import { memberDetails } from "@/utils/members";

export default class ConversationsCreateDm extends BaseCommand {
  static description = `Create a new DM conversation.

Creates a new direct message conversation with the specified user.
The recipient is an Ethereum address, or a handle such as nick#2341 with
--identifier-kind handle. An old handle of an inbox also finds it.

If a DM already exists with this user, the existing DM will be returned.
DMs are unique between two inbox IDs - you cannot have multiple DMs
with the same person.

Returns the DM's ID and details.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> <address>",
      description: "Create DM with an Ethereum address",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <address> --identifier-kind ethereum",
      description: "Explicitly specify identifier kind",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> nick#2341 --identifier-kind handle",
      description: "Create DM with the inbox that holds a handle",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <address> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static args = {
    identifier: Args.string({
      description: "The Ethereum address or handle of the recipient",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
    "identifier-kind": identifierKindFlag(),
  };

  async run(): Promise<void> {
    const { args, flags } = await this.parse(ConversationsCreateDm);
    const client = await this.initClient();

    const identity = publicIdentity(args.identifier, flags["identifier-kind"]);
    const dm = await reportUnresolvedHandles(client, [identity], () =>
      client.conversations.createDm(identity),
    );

    const state = await conversationState(dm);
    const members = await dm.members();

    this.output({
      id: dm.id,
      peerInboxId: await dm.peerInboxId(),
      createdAt: dm.createdAt.date.toISOString(),
      consentState: state.consentState,
      isActive: state.isActive,
      creatorInboxId: dm.creatorInboxId,
      members: await memberDetails(client, members, this.jsonOutput),
    });
  }
}
