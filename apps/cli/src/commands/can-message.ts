import { Args } from "@oclif/core";
import { Client } from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";
import {
  canMessageKey,
  identifierKindFlag,
  publicIdentity,
} from "@/utils/identity";

export default class CanMessage extends BaseCommand {
  static description = `Check if one or more identifiers can receive XMTP messages.

Queries the XMTP network to determine if the specified wallet addresses
have registered XMTP identities and can receive messages. With
--identifier-kind handle, the identifiers are handles such as nick#2341,
and a handle is reachable when an inbox holds it.

The command returns a mapping of each identifier to whether it can receive
XMTP messages (true/false).`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> <address>",
      description: "Check a single address",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <address-1> <address-2>",
      description: "Check multiple addresses",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> nick nick#2341 --identifier-kind handle",
      description: "Check whether inboxes hold two handles",
    },
    {
      command: "<%= config.bin %> <%= command.id %> <address> --json",
      description: "Output as JSON for scripting",
    },
  ];

  static strict = false; // Allow multiple positional args

  static args = {
    identifiers: Args.string({
      description: "Ethereum addresses or handles to check",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.commonFlags,
    "identifier-kind": identifierKindFlag(),
  };

  async run(): Promise<void> {
    const { argv, flags } = await this.parse(CanMessage);
    const identifiers = argv as string[];

    if (identifiers.length === 0) {
      this.error("At least one identifier is required");
    }

    const identifierObjects = identifiers.map((id) =>
      publicIdentity(id, flags["identifier-kind"]),
    );

    const results = await Client.canMessage(
      identifierObjects,
      this.networkOptions(),
    );

    const output = identifiers.map((id, index) => ({
      identifier: id,
      reachable: results.get(canMessageKey(identifierObjects[index])) ?? false,
    }));

    this.output(output);
  }
}
