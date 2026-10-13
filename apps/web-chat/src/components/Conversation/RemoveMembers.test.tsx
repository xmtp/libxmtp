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

import type { MemberProfile } from "@/helpers/member";

import { RemoveMembers } from "./RemoveMembers";

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
const member = {
  inboxId,
  handle: "current#1234",
  otherHandles: ["oldname"],
  permissionLevel: "member",
  identities: [],
  consentState: "unknown",
  address: "",
  avatar: null,
  description: null,
  displayName: null,
} as MemberProfile;
const open = (
  existingMembers = [member],
  removedMembers: MemberProfile[] = [],
  clientInboxId = "b".repeat(64),
) => {
  const ControlledMembers = () => {
    const [pendingRemovals, setPendingRemovals] = useState(removedMembers);
    return (
      <RemoveMembers
        existingMembers={existingMembers}
        removedMembers={pendingRemovals}
        clientInboxId={clientInboxId}
        onMembersRemoved={(next) => {
          setPendingRemovals(next);
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
  mocks.lookup.mockResolvedValue({ inboxId, handle: member.handle });
});
afterEach(cleanup);
it("removes a member by an old handle using its canonical inbox ID", async () => {
  open();
  fireEvent.change(
    screen.getByLabelText("Remove by address, inbox ID, or handle"),
    { target: { value: " @OldName " } },
  );
  const remove = screen.getByRole("button", { name: "Remove" });
  expect((remove as HTMLButtonElement).disabled).toBe(true);
  await waitFor(() =>
    expect((remove as HTMLButtonElement).disabled).toBe(false),
  );
  fireEvent.click(remove);
  expect(mocks.lookup).toHaveBeenCalledWith(
    "oldname",
    expect.objectContaining({ url: "http://127.0.0.1:5050" }),
  );
  expect(mocks.changed).toHaveBeenCalledExactlyOnceWith([member]);
  expect(screen.getByText("current#1234")).toBeTruthy();
  expect(screen.queryByText("oldname")).toBeNull();
});
it.each([
  {
    members: [],
    client: "b".repeat(64),
    removed: [],
    error: "This inbox is not a member of this group",
  },
  {
    members: [member],
    client: inboxId,
    removed: [],
    error: "You cannot remove yourself here",
  },
  {
    members: [{ ...member, permissionLevel: "superAdmin" as const }],
    client: "b".repeat(64),
    removed: [],
    error: "Remove this member's super admin role first",
  },
  {
    members: [member],
    client: "b".repeat(64),
    removed: [member],
    error: "This member is already in the removed list",
  },
])(
  "rejects removal when $error",
  async ({ members, client, removed, error }) => {
    open(members, removed, client);
    fireEvent.change(
      screen.getByLabelText("Remove by address, inbox ID, or handle"),
      { target: { value: "oldname" } },
    );
    await screen.findByText(error);
    expect(
      (
        screen.getByRole("button", {
          name: "Remove",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(mocks.changed).not.toHaveBeenCalled();
  },
);
it("restores a removed member by canonical inbox ID", () => {
  open([member], [member]);
  fireEvent.click(screen.getByRole("button", { name: "Restore" }));
  expect(mocks.changed).toHaveBeenCalledExactlyOnceWith([]);
});

it("refreshes a queued removal preview from the current verified member handle", () => {
  open([{ ...member, handle: "newname#1234" }], [member]);
  expect(screen.getByText("newname#1234")).toBeTruthy();
  expect(screen.queryByText("current#1234")).toBeNull();
});
