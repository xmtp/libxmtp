import { Flags } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import { IDENTIFIER_KINDS, publicIdentity } from "@/utils/identity";

export default class ClientInboxId extends BaseCommand {
  static description = `Fetch the inbox ID for an identifier.

Looks up the XMTP inbox ID associated with a given identifier (such as
an Ethereum address, or a handle such as nick#2341 with --kind handle).
First checks the local database cache, then queries the network if not
found locally.

This is useful for:
- Checking if an address is registered on XMTP
- Finding the inbox that holds a handle
- Looking up inbox IDs for other users
- Verifying identity associations

The output also shows the inbox's current handle and its other handles.
An old handle of an inbox finds it too; the output then shows the inbox's
current handle.

Returns null if the identifier has no associated inbox ID (not registered,
or no inbox holds the handle).`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> --identifier <address>",
      description: "Fetch inbox ID for an Ethereum address",
    },
    {
      command: "<%= config.bin %> <%= command.id %> -i <address> --json",
      description: "Fetch inbox ID with JSON output",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --identifier <address> --kind ethereum",
      description: "Explicitly specify identifier kind",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> --identifier nick#2341 --kind handle",
      description: "Fetch the inbox ID that holds a handle",
    },
  ];

  static flags = {
    ...BaseCommand.baseFlags,
    identifier: Flags.string({
      char: "i",
      description: "Identifier to look up (an Ethereum address or a handle)",
      required: true,
    }),
    kind: Flags.option({
      char: "k",
      options: IDENTIFIER_KINDS,
      description: "Type of identifier",
      default: "ethereum",
    })(),
  };

  async run(): Promise<void> {
    const { flags } = await this.parse(ClientInboxId);
    const client = await this.initClient();

    const inboxId = await client.inboxIdFor(
      publicIdentity(flags.identifier, flags.kind),
    );
    const states = inboxId ? await client.inboxStates([inboxId], true) : [];

    this.output({
      identifier: flags.identifier,
      kind: flags.kind,
      inboxId: inboxId ?? null,
      found: Boolean(inboxId),
      ...this.handleOutput(states.at(0)),
    });
  }
}
