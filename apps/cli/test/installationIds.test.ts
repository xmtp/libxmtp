import { Config } from "@oclif/core";
import { Client, type Client as XmtpClient } from "@xmtp/node-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import ClientRevokeInstallations from "@/commands/client/revoke-installations";
import InstallationAuthorized from "@/commands/installation-authorized";
import RevokeInstallations from "@/commands/revoke-installations";

const canonicalId = "ab".repeat(32);
const backend = { url: "http://127.0.0.1:5050" };
const walletKey = "01".repeat(32);

afterEach(() => vi.restoreAllMocks());

describe("installation ID command inputs", () => {
  it.each([canonicalId.toUpperCase(), `0x${canonicalId.toUpperCase()}`])(
    "normalizes root revocation input %s at the public SDK boundary",
    async (input) => {
      const command = new RevokeInstallations(
        ["inbox", "-i", input, "--force"],
        await Config.load({ root: process.cwd() }),
      );
      vi.spyOn(command, "getConfig").mockReturnValue({ walletKey });
      vi.spyOn(command, "networkOptions").mockReturnValue(backend);
      vi.spyOn(command, "output").mockImplementation(() => undefined);
      const revoke = vi
        .spyOn(Client, "revokeInstallations")
        .mockResolvedValue(undefined);

      await command.run();

      expect(revoke).toHaveBeenCalledOnce();
      expect(revoke).toHaveBeenCalledWith(
        expect.any(Object),
        "inbox",
        [canonicalId],
        backend,
      );
    },
  );

  it.each([canonicalId.toUpperCase(), `0x${canonicalId.toUpperCase()}`])(
    "normalizes client revocation input %s at the public SDK boundary",
    async (input) => {
      const command = new ClientRevokeInstallations(
        ["-i", input, "--force"],
        await Config.load({ root: process.cwd() }),
      );
      const revoke = vi.fn(async () => undefined);
      vi.spyOn(command, "initClient").mockResolvedValue({
        inboxId: "inbox",
        revokeInstallations: revoke,
      } as unknown as XmtpClient);
      vi.spyOn(command, "getConfig").mockReturnValue({ walletKey });
      vi.spyOn(command, "output").mockImplementation(() => undefined);

      await command.run();

      expect(revoke).toHaveBeenCalledOnce();
      expect(revoke).toHaveBeenCalledWith(expect.any(Object), [canonicalId]);
    },
  );

  it.each([canonicalId.toUpperCase(), `0x${canonicalId.toUpperCase()}`])(
    "normalizes authorization input %s at the public SDK boundary",
    async (input) => {
      const command = new InstallationAuthorized(
        ["inbox", input],
        await Config.load({ root: process.cwd() }),
      );
      vi.spyOn(command, "networkOptions").mockReturnValue(backend);
      vi.spyOn(command, "output").mockImplementation(() => undefined);
      const authorized = vi
        .spyOn(Client, "isInstallationAuthorized")
        .mockResolvedValue(true);

      await command.run();

      expect(authorized).toHaveBeenCalledWith("inbox", canonicalId, backend);
    },
  );

  it.each(["ab".repeat(31), "ab".repeat(33), "not-hex"])(
    "rejects malformed input %s before confirmation or a public SDK call",
    async (input) => {
      const config = await Config.load({ root: process.cwd() });
      const root = new RevokeInstallations(["inbox", "-i", input], config);
      vi.spyOn(root, "getConfig").mockReturnValue({ walletKey });
      vi.spyOn(root, "networkOptions").mockReturnValue(backend);
      const rootConfirm = vi
        .spyOn(root, "confirmAction")
        .mockResolvedValue(undefined);
      const rootRevoke = vi
        .spyOn(Client, "revokeInstallations")
        .mockResolvedValue(undefined);
      await expect(root.run()).rejects.toThrow();
      expect(rootConfirm).not.toHaveBeenCalled();
      expect(rootRevoke).not.toHaveBeenCalled();

      const local = new ClientRevokeInstallations(["-i", input], config);
      const localRevoke = vi.fn(async () => undefined);
      const init = vi.spyOn(local, "initClient").mockResolvedValue({
        revokeInstallations: localRevoke,
      } as unknown as XmtpClient);
      vi.spyOn(local, "getConfig").mockReturnValue({ walletKey });
      const localConfirm = vi
        .spyOn(local, "confirmAction")
        .mockResolvedValue(undefined);
      await expect(local.run()).rejects.toThrow();
      expect(init).not.toHaveBeenCalled();
      expect(localConfirm).not.toHaveBeenCalled();
      expect(localRevoke).not.toHaveBeenCalled();

      const query = new InstallationAuthorized(["inbox", input], config);
      vi.spyOn(query, "networkOptions").mockReturnValue(backend);
      const authorized = vi
        .spyOn(Client, "isInstallationAuthorized")
        .mockResolvedValue(true);
      await expect(query.run()).rejects.toThrow();
      expect(authorized).not.toHaveBeenCalled();
    },
  );
});
