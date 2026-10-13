import { Args, Flags } from "@oclif/core";

import { BaseCommand } from "@/baseCommand";
import {
  asciiLowercase,
  claimFailureMessage,
  formatOtherHandles,
  handleFields,
  invalidHandleMessage,
  isValidHandle,
} from "@/utils/handles";

export default class ClientHandleClaim extends BaseCommand {
  static description = `Claim a handle, or switch back to one of your other handles.

A handle is a public name, such as nick or nick#2341, that people on this
backend use to find you and send you messages. The handle you claim becomes
your current handle. Your earlier handles stay yours, and people can still
find you by them.

Handles are public and permanent. You can change your handle once every
7 days, up to 64 times in total. A switch back to one of your other handles
counts as a change.

The backend decides each claim. If the handle is taken, not allowed, or you
changed your handle too recently, the command fails and claims nothing. It
never claims a different handle. Run 'xmtp client handle-available <name>'
to find free handles. If the handle is already your current handle, the
command fails before it asks the backend, and claims nothing.`;

  static examples = [
    {
      command: "<%= config.bin %> <%= command.id %> nick#2341",
      description: "Claim a handle",
    },
    {
      command: "<%= config.bin %> <%= command.id %> nick --force --json",
      description: "Claim a handle without a confirmation prompt",
    },
  ];

  static args = {
    handle: Args.string({
      description: "The handle to claim, such as nick or nick#2341",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
    force: Flags.boolean({
      description: "Skip confirmation prompt",
      default: false,
    }),
  };

  async run(): Promise<void> {
    const { args, flags } = await this.parse(ClientHandleClaim);
    // Check the grammar before the prompt or any output shows the input:
    // text outside it, such as an escape sequence, never reaches the terminal.
    if (!isValidHandle(args.handle)) {
      this.error(invalidHandleMessage(args.handle));
    }
    const handle = asciiLowercase(args.handle);
    const client = await this.initClient();

    // The client loaded the inbox's identity updates when it was created.
    const before = await client.inboxState(false);
    // The backend admits a claim of the current handle. It changes nothing,
    // but it uses one of the 64 changes and starts the 7-day wait again.
    if (handle === before.handle) {
      this.error(
        `${handle} is already your current handle. Nothing was claimed.`,
      );
    }
    const kept = before.handle
      ? ` ${before.handle} stays yours, and people can still find you by it.`
      : "";
    await this.confirmAction(
      `Handles are public and permanent. People will see ${handle} as your handle.${kept} You cannot change your handle again for 7 days.`,
      flags.force,
    );

    try {
      await client.claimHandle(args.handle);
    } catch (error) {
      const message = claimFailureMessage(
        error,
        args.handle,
        before.otherHandles,
      );
      if (message !== undefined) {
        this.error(message);
      }
      throw error;
    }

    const after = await client.inboxState(false);
    const fields = handleFields(after);

    if (this.jsonOutput) {
      this.output({ inboxId: after.inboxId, ...fields });
      return;
    }

    const lines = [`Your handle is now ${String(fields.handle)}.`];
    if (fields.otherHandles.length > 0) {
      lines.push(`Other handles: ${formatOtherHandles(fields.otherHandles)}`);
    }
    this.log(lines.join("\n"));
  }
}
