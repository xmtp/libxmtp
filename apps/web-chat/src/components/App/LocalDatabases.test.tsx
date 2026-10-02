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
  const selected = "/xmtp/selected-backend/deployment/inbox/xmtp.db3";
  const retained = "/xmtp/selected-backend/deployment/other/xmtp.db3";
  const foreign = "/xmtp/other-backend/deployment/inbox/xmtp.db3";
  const foreignInbox =
    "/xmtp/other-backend/deployment/selected-backend/xmtp.db3";
  mocks.list.mockResolvedValue([selected, retained, foreign, foreignInbox]);
  mocks.admin.mockResolvedValue({
    listFiles: mocks.list,
    deleteFile: mocks.remove,
    end: mocks.end,
  });
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
  expect(
    [...select.querySelectorAll("option")].map((option) => option.value),
  ).toEqual(["", retained]);
});
