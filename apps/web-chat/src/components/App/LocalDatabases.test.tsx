import { MantineProvider } from "@mantine/core";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { LocalDatabases } from "./LocalDatabases";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  remove: vi.fn(),
  end: vi.fn(),
  admin: vi.fn(),
}));
vi.mock("@xmtp/browser-sdk", () => ({
  Storage: { admin: mocks.admin },
  XmtpError: { StorageBusy: class extends Error {} },
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ backendUrl: "https://example.com" }),
}));
vi.mock("@/helpers/backend", () => ({
  backendLabel: async () => "selected-backend",
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("deletes only a selected database for this backend and awaits admin end", async () => {
  const deployment = `deletion-test-${crypto.randomUUID()}`;
  const selected = `/xmtp-sdk/selected-backend/${deployment}/inbox/xmtp.db3`;
  const retained = `/xmtp-sdk/selected-backend/${deployment}/other/xmtp.db3`;
  const foreign = `/xmtp-sdk/other-backend/${deployment}/inbox/xmtp.db3`;
  const foreignInbox = `/xmtp-sdk/other-backend/${deployment}/selected-backend/xmtp.db3`;
  const legacy = `xmtp-selected-backend-${deployment}.db3`;
  const root = await navigator.storage.getDirectory();
  const makeDirectory = async (path: string) => {
    let directory = root;
    for (const segment of path.split("/").filter(Boolean)) {
      directory = await directory.getDirectoryHandle(segment, { create: true });
    }
  };
  const selectedFiles = `xmtp-sdk/selected-backend/${deployment}/inbox/attachments`;
  const retainedFiles = `xmtp-sdk/selected-backend/${deployment}/other/attachments`;
  const legacyFiles = `${legacy}.attachments`;
  await Promise.all([
    makeDirectory(selectedFiles),
    makeDirectory(retainedFiles),
    makeDirectory(legacyFiles),
  ]);
  mocks.list.mockResolvedValue([
    selected,
    retained,
    foreign,
    foreignInbox,
    legacy,
  ]);
  mocks.admin.mockResolvedValue({
    listFiles: mocks.list,
    deleteFile: mocks.remove,
    end: mocks.end,
  });
  try {
    render(
      <MantineProvider>
        <LocalDatabases />
      </MantineProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Local databases" }));
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(1));
    const remove = screen.getByRole("button", {
      name: "Delete selected database",
    }) as HTMLButtonElement;
    expect(remove.disabled).toBe(true);
    const select = screen.getByRole("combobox", { name: "Database" });
    expect(
      [...select.querySelectorAll("option")].map((option) => option.value),
    ).toEqual(["", selected, retained, legacy]);
    fireEvent.change(select, { target: { value: selected } });
    fireEvent.click(remove);
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(2));
    expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(selected);
    await expect(
      root
        .getDirectoryHandle("xmtp-sdk")
        .then((sdk) => sdk.getDirectoryHandle("selected-backend"))
        .then((backend) => backend.getDirectoryHandle(deployment))
        .then((path) => path.getDirectoryHandle("inbox"))
        .then((inbox) => inbox.getDirectoryHandle("attachments")),
    ).rejects.toMatchObject({ name: "NotFoundError" });
    expect(
      [...select.querySelectorAll("option")].map((option) => option.value),
    ).toEqual(["", retained, legacy]);
    await expect(
      root
        .getDirectoryHandle("xmtp-sdk")
        .then((sdk) => sdk.getDirectoryHandle("selected-backend"))
        .then((backend) => backend.getDirectoryHandle(deployment))
        .then((path) => path.getDirectoryHandle("other"))
        .then((other) => other.getDirectoryHandle("attachments")),
    ).resolves.toBeDefined();

    fireEvent.change(select, { target: { value: legacy } });
    fireEvent.click(remove);
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(3));
    expect(mocks.remove).toHaveBeenLastCalledWith(legacy);
    await expect(root.getDirectoryHandle(legacyFiles)).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    const sdk = await root.getDirectoryHandle("xmtp-sdk");
    const backend = await sdk.getDirectoryHandle("selected-backend");
    await backend.removeEntry(deployment, { recursive: true });
    await root.removeEntry(legacyFiles, { recursive: true }).catch(() => {});
  }
});

it("retries attachment cleanup after the database file is deleted", async () => {
  const deployment = `cleanup-retry-${crypto.randomUUID()}`;
  const selected = `/xmtp-sdk/selected-backend/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("selected-backend", {
    create: true,
  });
  const directory = await backend.getDirectoryHandle(deployment, {
    create: true,
  });
  const inbox = await directory.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  mocks.list
    .mockResolvedValueOnce([selected])
    .mockResolvedValueOnce([selected])
    .mockResolvedValue([]);
  mocks.admin.mockResolvedValue({
    listFiles: mocks.list,
    deleteFile: mocks.remove,
    end: mocks.end,
  });
  const storageSpy = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS cleanup failed"))
    .mockResolvedValue(root);
  try {
    render(
      <MantineProvider>
        <LocalDatabases />
      </MantineProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Local databases" }));
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByRole("combobox", { name: "Database" }), {
      target: { value: selected },
    });
    const remove = screen.getByRole("button", {
      name: "Delete selected database",
    });
    fireEvent.click(remove);
    await screen.findByText("OPFS cleanup failed");
    expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(selected);
    await expect(
      inbox.getDirectoryHandle("attachments"),
    ).resolves.toBeDefined();

    fireEvent.click(remove);
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(3));
    expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(selected);
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      {
        name: "NotFoundError",
      },
    );
  } finally {
    storageSpy.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
  }
});
