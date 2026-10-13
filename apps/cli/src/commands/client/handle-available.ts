import { Args } from "@oclif/core";
import { XmtpError } from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";
import { asciiLowercase, invalidHandleMessage } from "@/utils/handles";

export default class ClientHandleAvailable extends BaseCommand {
  static description = `List free handles for a name.

A handle is a public name, such as nick or nick#2341, that people on this
backend use to find you and send you messages.

For a name without a code, such as nick, the backend checks the bare name and
100 random codes in one lookup. The output lists the free handles, with the
bare name first when it is free. For a handle with a code, such as nick#2341,
only that handle is checked.

The backend decides which handles you can claim. A free handle can still be
refused when you claim it. Run 'xmtp client handle-claim <handle>' to claim
one.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> nick",
      description: "List free handles for the name nick",
    },
    {
      command: "<%= config.bin %> <%= command.id %> nick#2341",
      description: "Check one handle with a code",
    },
    {
      command: "<%= config.bin %> <%= command.id %> nick --json",
      description: "Output as JSON for scripting",
    },
  ];

  static args = {
    name: Args.string({
      description: "The name or handle to check, such as nick or nick#2341",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
  };

  async run(): Promise<void> {
    const { args } = await this.parse(ClientHandleAvailable);
    const client = await this.initClient();

    let handles: string[];
    try {
      handles = await client.availableHandles(args.name);
    } catch (error) {
      if (error instanceof XmtpError.InvalidArgument) {
        this.error(invalidHandleMessage(args.name));
      }
      throw error;
    }

    if (this.jsonOutput) {
      this.output({ name: args.name, handles });
      return;
    }

    if (handles.length > 0) {
      this.output(handles);
      return;
    }

    const name = asciiLowercase(args.name);
    this.log(
      name.includes("#")
        ? `${name} is taken.`
        : `${name} and the codes checked are all taken. Run the command again, or try a different name.`,
    );
  }
}
