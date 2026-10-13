import { MantineProvider } from "@mantine/core";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type * as SDK from "@xmtp/browser-sdk";
import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CreateGroupModal } from "@/components/Conversations/CreateGroupModal";

import { ManageMembersModal } from "./ManageMembersModal";
import type { MembersProps } from "./Members";
const mocks = vi.hoisted(() => {
  class Group {
    id = "group-1";
    addMembers = vi.fn(async () => {});
    removeMembers = vi.fn(async () => {});
  }
  const group = new Group();
  return {
    Group,
    group,
    createGroup: vi.fn(),
    addConversation: vi.fn(),
    navigate: vi.fn(),
  };
});
vi.mock("@xmtp/browser-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof SDK>()),
  Group: mocks.Group,
}));
vi.mock("react-router", () => ({
  useNavigate: () => mocks.navigate,
  useOutletContext: () => ({ conversationId: "group-1" }),
}));
vi.mock("@/hooks/useConversation", () => ({
  useConversation: () => ({ conversation: mocks.group, members: new Map() }),
}));
vi.mock("@/hooks/useConversations", () => ({
  useConversations: () => ({ createGroup: mocks.createGroup }),
}));
vi.mock("@/hooks/useClientPermissions", () => ({
  useClientPermissions: () => ({ canAddMembers: true, canRemoveMembers: true }),
}));
vi.mock("@/hooks/useCollapsedMediaQuery", () => ({
  useCollapsedMediaQuery: () => false,
}));
vi.mock("@/stores/inbox/hooks", () => ({
  useActions: () => ({ addConversation: mocks.addConversation }),
}));
vi.mock("@/components/Modal", () => ({
  Modal: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/layouts/ContentLayout", () => ({
  ContentLayout: ({
    children,
    footer,
  }: {
    children: ReactNode;
    footer: ReactNode;
  }) => (
    <div>
      {children}
      {footer}
    </div>
  ),
}));
vi.mock("@/components/Conversation/Metadata", () => ({ Metadata: () => null }));
vi.mock("@/components/Conversation/Permissions", () => ({
  Permissions: () => null,
  defaultPolicySet: {},
}));
vi.mock("@/components/Conversation/Members", () => ({
  Members: ({ onMembersAdded, onMembersRemoved }: MembersProps) => (
    <button
      onClick={() => {
        onMembersAdded?.([
          {
            inboxId: "a".repeat(64),
            address: "0x" + "1".repeat(40),
            handle: null,
            displayName: null,
            description: null,
            avatar: null,
          },
          {
            inboxId: "b".repeat(64),
            address: "",
            handle: "nick#2341",
            displayName: null,
            description: null,
            avatar: null,
          },
        ]);
        onMembersRemoved?.([
          {
            inboxId: "c".repeat(64),
            handle: "removed#1234",
            otherHandles: [],
            identities: [],
            consentState: "unknown",
            permissionLevel: "member",
            address: "",
            displayName: null,
            description: null,
            avatar: null,
          },
        ]);
      }}>
      Choose recipients
    </button>
  ),
}));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.createGroup.mockResolvedValue(mocks.group);
});
afterEach(cleanup);
it("creates a group once with resolved inbox IDs for both addresses and handles", async () => {
  render(
    <MantineProvider>
      <CreateGroupModal />
    </MantineProvider>,
  );
  fireEvent.click(screen.getByText("Choose recipients"));
  fireEvent.click(screen.getByRole("button", { name: "Create" }));
  await waitFor(() =>
    expect(mocks.navigate).toHaveBeenCalledWith("/conversations/group-1"),
  );
  expect(mocks.createGroup).toHaveBeenCalledExactlyOnceWith(
    ["a".repeat(64), "b".repeat(64)],
    expect.any(Object),
  );
  expect(mocks.group.addMembers).not.toHaveBeenCalled();
});
it("adds and removes members using only canonical inbox IDs", async () => {
  render(
    <MantineProvider>
      <ManageMembersModal />
    </MantineProvider>,
  );
  fireEvent.click(screen.getByText("Choose recipients"));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(mocks.navigate).toHaveBeenCalledWith("/conversations/group-1"),
  );
  expect(mocks.group.addMembers).toHaveBeenCalledExactlyOnceWith([
    "a".repeat(64),
    "b".repeat(64),
  ]);
  expect(mocks.group.removeMembers).toHaveBeenCalledExactlyOnceWith([
    "c".repeat(64),
  ]);
});
