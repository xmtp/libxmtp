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
  backendUrl: "https://example.com",
  list: vi.fn(),
  remove: vi.fn(),
  end: vi.fn(),
  admin: vi.fn(),
  fetchServerConfiguration: vi.fn().mockResolvedValue({
    identifier: "selected-deployment",
  }),
}));
vi.mock("@xmtp/browser-sdk", () => ({
  Client: { fetchServerConfiguration: mocks.fetchServerConfiguration },
  Storage: { admin: mocks.admin },
  XmtpError: { StorageBusy: class extends Error {} },
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ backendUrl: mocks.backendUrl }),
}));
vi.mock("@/helpers/backend", () => ({
  backendLabel: async () => "selected-backend",
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  mocks.backendUrl = "https://example.com";
  mocks.fetchServerConfiguration.mockResolvedValue({
    identifier: "selected-deployment",
  });
});

const deploymentComponent = async (name: string) => {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(name),
  );
  return `${name}-${Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")}`;
};

it("deletes only a selected database for this backend and awaits admin end", async () => {
  const deployment = await deploymentComponent("selected-deployment");
  const selectedInbox = "a".repeat(64);
  const retainedInbox = "b".repeat(64);
  const selected = `/xmtp-sdk/selected-backend/${deployment}/${selectedInbox}/xmtp.db3`;
  const retained = `/xmtp-sdk/selected-backend/${deployment}/${retainedInbox}/xmtp.db3`;
  const foreign = `/xmtp-sdk/other-backend/${deployment}/${selectedInbox}/xmtp.db3`;
  const foreignInbox = `/xmtp-sdk/other-backend/${deployment}/${retainedInbox}/xmtp.db3`;
  const unrelated = `/xmtp-sdk/selected-backend/arbitrary/arbitrary/xmtp.db3`;
  const inboxId = crypto.randomUUID().replaceAll("-", "").repeat(2);
  const legacy = `xmtp-selected-backend-${inboxId}.db3`;
  const nestedLegacy = `/unrelated/${legacy}`;
  const root = await navigator.storage.getDirectory();
  const makeDirectory = async (path: string) => {
    let directory = root;
    for (const segment of path.split("/").filter(Boolean)) {
      directory = await directory.getDirectoryHandle(segment, { create: true });
    }
  };
  const selectedFiles = `xmtp-sdk/selected-backend/${deployment}/${selectedInbox}/attachments`;
  const retainedFiles = `xmtp-sdk/selected-backend/${deployment}/${retainedInbox}/attachments`;
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
    unrelated,
    legacy,
    nestedLegacy,
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
    ).toEqual(["", selected, retained]);
    fireEvent.change(select, { target: { value: selected } });
    fireEvent.click(remove);
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(2));
    expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(selected);
    await expect(
      root
        .getDirectoryHandle("xmtp-sdk")
        .then((sdk) => sdk.getDirectoryHandle("selected-backend"))
        .then((backend) => backend.getDirectoryHandle(deployment))
        .then((path) => path.getDirectoryHandle(selectedInbox))
        .then((inbox) => inbox.getDirectoryHandle("attachments")),
    ).rejects.toMatchObject({ name: "NotFoundError" });
    expect(
      [...select.querySelectorAll("option")].map((option) => option.value),
    ).toEqual(["", retained]);
    await expect(
      root
        .getDirectoryHandle("xmtp-sdk")
        .then((sdk) => sdk.getDirectoryHandle("selected-backend"))
        .then((backend) => backend.getDirectoryHandle(deployment))
        .then((path) => path.getDirectoryHandle(retainedInbox))
        .then((other) => other.getDirectoryHandle("attachments")),
    ).resolves.toBeDefined();

    expect(mocks.remove).not.toHaveBeenCalledWith(legacy);
    await expect(root.getDirectoryHandle(legacyFiles)).resolves.toBeDefined();
  } finally {
    const sdk = await root.getDirectoryHandle("xmtp-sdk");
    const backend = await sdk.getDirectoryHandle("selected-backend");
    await backend.removeEntry(deployment, { recursive: true });
    await root.removeEntry(legacyFiles, { recursive: true }).catch(() => {});
  }
});

it("does not list or delete a database from another deployment at the same origin", async () => {
  mocks.backendUrl = "https://example.com/b";
  const selectedDeployment = await deploymentComponent("selected-deployment");
  const otherDeployment = await deploymentComponent("other-deployment");
  const inboxId = "a".repeat(64);
  const selected = `/xmtp-sdk/selected-backend/${selectedDeployment}/${inboxId}/xmtp.db3`;
  const other = `/xmtp-sdk/selected-backend/${otherDeployment}/${inboxId}/xmtp.db3`;
  const forgedDeployment = `forged-${selectedDeployment.slice("selected-deployment-".length)}`;
  const forged = `/xmtp-sdk/selected-backend/${forgedDeployment}/${inboxId}/xmtp.db3`;
  const ambiguousLegacy = `xmtp-selected-backend-${inboxId}.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("selected-backend", {
    create: true,
  });
  const selectedDir = await backend.getDirectoryHandle(selectedDeployment, {
    create: true,
  });
  const selectedInbox = await selectedDir.getDirectoryHandle(inboxId, {
    create: true,
  });
  await selectedInbox.getDirectoryHandle("attachments", { create: true });
  const otherDir = await backend.getDirectoryHandle(otherDeployment, {
    create: true,
  });
  const otherInbox = await otherDir.getDirectoryHandle(inboxId, {
    create: true,
  });
  await otherInbox.getDirectoryHandle("attachments", { create: true });
  const forgedDir = await backend.getDirectoryHandle(forgedDeployment, {
    create: true,
  });
  const forgedInbox = await forgedDir.getDirectoryHandle(inboxId, {
    create: true,
  });
  await forgedInbox.getDirectoryHandle("attachments", { create: true });
  mocks.list.mockResolvedValue([selected, other, forged, ambiguousLegacy]);
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
    const select = screen.getByRole("combobox", { name: "Database" });
    expect(
      [...select.querySelectorAll("option")].map((option) => option.value),
    ).toEqual(["", selected]);

    fireEvent.change(select, { target: { value: selected } });
    fireEvent.click(
      screen.getByRole("button", { name: "Delete selected database" }),
    );
    await waitFor(() => expect(mocks.end).toHaveBeenCalledTimes(2));
    expect(mocks.remove).toHaveBeenCalledWith(selected);
    expect(mocks.remove).not.toHaveBeenCalledWith(other);
    expect(mocks.remove).not.toHaveBeenCalledWith(forged);
    expect(mocks.remove).not.toHaveBeenCalledWith(ambiguousLegacy);
    expect(localStorage.getItem("XMTP_PENDING_DATABASE_DELETION")).toBeNull();
    await expect(
      otherInbox.getDirectoryHandle("attachments"),
    ).resolves.toBeDefined();
    await expect(
      forgedInbox.getDirectoryHandle("attachments"),
    ).resolves.toBeDefined();
  } finally {
    await backend.removeEntry(selectedDeployment, { recursive: true });
    await backend.removeEntry(otherDeployment, { recursive: true });
    await backend.removeEntry(forgedDeployment, { recursive: true });
  }
});

it("does not offer deletion when the selected deployment cannot be resolved", async () => {
  mocks.fetchServerConfiguration.mockRejectedValueOnce(
    new Error("Server configuration unavailable"),
  );
  render(
    <MantineProvider>
      <LocalDatabases />
    </MantineProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Local databases" }));
  await screen.findByText("Server configuration unavailable");
  expect(mocks.admin).not.toHaveBeenCalled();
  expect(mocks.remove).not.toHaveBeenCalled();
  expect(
    (
      screen.getByRole("button", {
        name: "Delete selected database",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
});

it("does not retry an ambiguous legacy deletion intent", async () => {
  mocks.backendUrl = "https://example.com/b";
  const legacy = `xmtp-selected-backend-${"a".repeat(64)}.db3`;
  localStorage.setItem(
    "XMTP_PENDING_DATABASE_DELETION",
    JSON.stringify([legacy]),
  );
  mocks.list.mockResolvedValue([legacy]);
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
    await waitFor(() => expect(mocks.end).toHaveBeenCalled());
    expect(mocks.remove).not.toHaveBeenCalled();
    expect(localStorage.getItem("XMTP_PENDING_DATABASE_DELETION")).toBe(
      JSON.stringify([legacy]),
    );
  } finally {
    localStorage.removeItem("XMTP_PENDING_DATABASE_DELETION");
  }
});

it("does not retry a database deletion outside recorded deployments", async () => {
  mocks.backendUrl = "https://example.com/b";
  const selectedDeployment = await deploymentComponent("selected-deployment");
  const forged = `xmtp-sdk/selected-backend/forged-${selectedDeployment.slice("selected-deployment-".length)}/${"a".repeat(64)}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("selected-backend", {
    create: true,
  });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com/b": "selected-deployment" },
    }),
  );
  await writer.close();
  localStorage.setItem(
    "XMTP_PENDING_DATABASE_DELETION",
    JSON.stringify([forged]),
  );
  mocks.list.mockResolvedValue([forged]);
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
    await waitFor(() => expect(mocks.end).toHaveBeenCalled());
    expect(mocks.remove).not.toHaveBeenCalled();
    expect(localStorage.getItem("XMTP_PENDING_DATABASE_DELETION")).toBe(
      JSON.stringify([forged]),
    );
  } finally {
    localStorage.removeItem("XMTP_PENDING_DATABASE_DELETION");
    await backend.removeEntry("deployments.json");
  }
});

it("keeps the database listed until attachment cleanup succeeds", async () => {
  const deployment = await deploymentComponent("selected-deployment");
  const inboxId = "c".repeat(64);
  const selected = `/xmtp-sdk/selected-backend/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("selected-backend", {
    create: true,
  });
  const directory = await backend.getDirectoryHandle(deployment, {
    create: true,
  });
  const inbox = await directory.getDirectoryHandle(inboxId, { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  mocks.list
    .mockResolvedValueOnce([selected])
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
    expect(mocks.remove).not.toHaveBeenCalled();
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

it("keeps the database when the deletion intent cannot be saved", async () => {
  const deployment = await deploymentComponent("selected-deployment");
  const selected = `/xmtp-sdk/selected-backend/${deployment}/${"e".repeat(64)}/xmtp.db3`;
  mocks.list.mockResolvedValue([selected]);
  mocks.admin.mockResolvedValue({
    listFiles: mocks.list,
    deleteFile: mocks.remove,
    end: mocks.end,
  });
  const setItem = Storage.prototype.setItem;
  const storageSpy = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(function (this: Storage, key, value) {
      if (key === "XMTP_PENDING_DATABASE_DELETION") {
        throw new Error("Deletion intent could not be saved");
      }
      return setItem.call(this, key, value);
    });
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
    fireEvent.click(
      screen.getByRole("button", { name: "Delete selected database" }),
    );
    await screen.findByText("Deletion intent could not be saved");
    expect(mocks.remove).not.toHaveBeenCalled();
  } finally {
    storageSpy.mockRestore();
  }
});

it("retries a saved deletion intent after the database delete fails", async () => {
  const deployment = await deploymentComponent("selected-deployment");
  const inboxId = "f".repeat(64);
  const selected = `/xmtp-sdk/selected-backend/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("selected-backend", {
    create: true,
  });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com": "selected-deployment" },
    }),
  );
  await writer.close();
  const directory = await backend.getDirectoryHandle(deployment, {
    create: true,
  });
  const inbox = await directory.getDirectoryHandle(inboxId, { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  mocks.list
    .mockResolvedValueOnce([selected])
    .mockResolvedValueOnce([selected])
    .mockResolvedValueOnce([selected])
    .mockResolvedValue([]);
  mocks.remove
    .mockRejectedValueOnce(new Error("Database delete failed"))
    .mockResolvedValueOnce(true);
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
    fireEvent.change(screen.getByRole("combobox", { name: "Database" }), {
      target: { value: selected },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Delete selected database" }),
    );
    await screen.findByText("Database delete failed");
    expect(localStorage.getItem("XMTP_PENDING_DATABASE_DELETION")).toContain(
      selected,
    );
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      { name: "NotFoundError" },
    );

    cleanup();
    render(
      <MantineProvider>
        <LocalDatabases />
      </MantineProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Local databases" }));
    await waitFor(() => expect(mocks.remove).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(localStorage.getItem("XMTP_PENDING_DATABASE_DELETION")).toBeNull(),
    );
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      {
        name: "NotFoundError",
      },
    );
  } finally {
    localStorage.removeItem("XMTP_PENDING_DATABASE_DELETION");
    await backend.removeEntry(deployment, { recursive: true });
    await backend.removeEntry("deployments.json");
  }
});
