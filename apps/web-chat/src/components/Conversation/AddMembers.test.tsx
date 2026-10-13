import { MantineProvider } from "@mantine/core";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { AddMembers, type PendingMember } from "./AddMembers";
const mocks = vi.hoisted(() => ({
  lookup: vi.fn(),
  address: vi.fn(),
  changed: vi.fn(),
  source: () => ({
    credential: async () => ({ value: "", expiresAtSeconds: 0n }),
  }),
}));
vi.mock("@/helpers/inboxId", () => ({
  getInboxIdForHandleQuery: mocks.lookup,
  getInboxIdForAddressQuery: mocks.address,
}));
vi.mock("@/contexts/AuthTokenContext", () => ({
  useAuthToken: () => ({ createCredentialSource: mocks.source }),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ backendUrl: "http://127.0.0.1:5050" }),
}));
const inboxId = "a".repeat(64);
const pending = {
  inboxId,
  address: "",
  handle: "nick#2341",
  avatar: null,
  description: null,
  displayName: null,
};
const open = (addedMembers: PendingMember[] = []) => {
  const ControlledMembers = () => {
    const [pendingMembers, setPendingMembers] = useState(addedMembers);
    return (
      <AddMembers
        existingMembers={[]}
        addedMembers={pendingMembers}
        onMembersAdded={(next) => {
          setPendingMembers(next);
          mocks.changed(next);
        }}
      />
    );
  };
  return render(
    <MantineProvider>
      <ControlledMembers />
    </MantineProvider>,
  );
};
beforeEach(() => {
  vi.clearAllMocks();
  mocks.lookup.mockResolvedValue({ inboxId, handle: "nick#2341" });
});
afterEach(cleanup);
it("adds an old handle as a canonical inbox ID and previews its current handle", async () => {
  open();
  fireEvent.change(screen.getByLabelText("Address, inbox ID, or handle"), {
    target: { value: " @OldNick " },
  });
  const add = screen.getByRole("button", { name: "Add" });
  expect((add as HTMLButtonElement).disabled).toBe(true);
  await waitFor(() => expect((add as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(add);
  expect(mocks.changed).toHaveBeenCalledExactlyOnceWith([pending]);
  expect(screen.getByText("nick#2341")).toBeTruthy();
  expect(screen.queryByText("oldnick")).toBeNull();
});
it("deduplicates an old handle against the same pending inbox", async () => {
  open([pending]);
  fireEvent.change(screen.getByLabelText("Address, inbox ID, or handle"), {
    target: { value: "oldnick" },
  });
  await screen.findByText("This member is already in the list");
  expect(
    (
      screen.getByRole("button", {
        name: "Add",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(mocks.changed).not.toHaveBeenCalled();
});
