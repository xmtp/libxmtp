import { MantineProvider } from "@mantine/core";
import { cleanup, render, screen } from "@testing-library/react";
import type * as SDK from "@xmtp/browser-sdk";
import type { Member } from "@xmtp/browser-sdk";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { Conversation } from "@/components/Conversation/Conversation";
import { HandleText } from "@/components/Handles/HandleText";

import { ConversationCard } from "./ConversationCard";

const mocks = vi.hoisted(() => {
  class Base {
    id = "conversation-1";
    state = async () => ({ isActive: false });
    messages = async () => [];
  }
  class Dm extends Base {}
  class Group extends Base {}
  return {
    Dm,
    Group,
    conversation: new Dm() as Base,
    name: "nick#2341",
    members: new Map<string, Member>(),
    addMessages: vi.fn(),
  };
});
vi.mock("@xmtp/browser-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof SDK>()),
  Dm: mocks.Dm,
  Group: mocks.Group,
}));
vi.mock("@/stores/inbox/hooks", () => ({
  useActions: () => ({ addMessages: mocks.addMessages }),
  useConversation: () => mocks.conversation,
  useMembers: () => mocks.members,
  useMessages: () => [],
  useMetadata: () => ({ name: mocks.name }),
  usePermissions: () => ({}),
}));
vi.mock("@/components/Conversation/ConversationMenu", () => ({
  ConversationMenu: () => null,
}));
vi.mock("@/components/Conversation/MembersList", () => ({
  MembersList: () => null,
}));
vi.mock("@/components/Conversation/Composer", () => ({ Composer: () => null }));
vi.mock("@/components/Messages/Messages", () => ({ Messages: () => null }));
vi.mock("@/contexts/XMTPContext", () => ({ useClient: () => ({}) }));
vi.mock("@/hooks/useConsentState", () => ({
  useConsentState: () => undefined,
}));
vi.mock("@/layouts/ContentLayout", () => ({
  ContentLayout: ({
    title,
    children,
  }: {
    title: ReactNode;
    children: ReactNode;
  }) => (
    <div>
      <h1>{title}</h1>
      {children}
    </div>
  ),
}));
const peer = "a".repeat(64);
const member: Member = {
  inboxId: peer,
  identities: [],
  consentState: "unknown",
  permissionLevel: "member",
  handle: "nick#2341",
  otherHandles: ["oldnick"],
};
const open = (children: ReactNode) =>
  render(
    <MantineProvider>
      <MemoryRouter>{children}</MemoryRouter>
    </MantineProvider>,
  );
beforeEach(() => {
  vi.clearAllMocks();
  mocks.conversation = new mocks.Dm();
  mocks.name = "nick#2341";
  mocks.members = new Map([[peer, member]]);
});
afterEach(cleanup);
it("shows the current DM handle with @ in the conversation list and header", () => {
  open(
    <>
      <ConversationCard conversationId="conversation-1" />
      <Conversation conversationId="conversation-1" />
    </>,
  );
  expect(screen.getByRole("heading").textContent).toBe("@nick#2341");
  expect(screen.getAllByText("@nick")).toHaveLength(2);
  expect(mocks.name).toBe("nick#2341");
});
it("keeps custom group names intact even when a member has that handle", () => {
  mocks.conversation = new mocks.Group();
  open(
    <>
      <ConversationCard conversationId="conversation-1" />
      <Conversation conversationId="conversation-1" />
    </>,
  );
  expect(screen.getByRole("heading").textContent).toBe("nick#2341");
  expect(screen.queryByText("@nick")).toBeNull();
});
it("preserves a custom DM label instead of prefixing it", () => {
  mocks.name = "Family";
  open(<Conversation conversationId="conversation-1" />);
  expect(screen.getByRole("heading").textContent).toBe("Family");
});
it("shows the inbox ID unchanged when a DM peer has no handle", () => {
  mocks.members = new Map([[peer, { ...member, handle: undefined }]]);
  mocks.name = peer;
  open(<Conversation conversationId="conversation-1" />);
  expect(screen.getByRole("heading").textContent).toBe(peer);
});
it("does not duplicate @ when rendering an already prefixed handle", () => {
  open(
    <h1>
      <HandleText handle="@nick#2341" withPrefix />
    </h1>,
  );
  expect(screen.getByRole("heading").textContent).toBe("@nick#2341");
});
