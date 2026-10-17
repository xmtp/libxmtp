import { MantineProvider } from "@mantine/core";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Member, Message as XmtpMessage } from "@xmtp/browser-sdk";
import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ManageDetailsModal } from "@/components/Conversation/ManageDetailsModal";
import { MemberPopover } from "@/components/Conversation/MemberPopover";

import { Message } from "./Message";
import { MessageProperties } from "./MessageProperties";
import { ReplyContent } from "./ReplyContent";

const mocks = vi.hoisted(() => ({
  members: new Map<string, Member>(),
  messages: [] as XmtpMessage[],
  conversation: {
    id: "group-1",
    addedByInboxId: "a".repeat(64),
    state: async () => ({ consentState: "allowed" }),
  },
  react: vi.fn(),
  navigate: vi.fn(),
  scroll: vi.fn(),
  setReplyTarget: vi.fn(),
}));
vi.mock("@/hooks/useConversation", () => ({
  useConversation: () => ({
    conversation: mocks.conversation,
    members: mocks.members,
    messages: mocks.messages,
    permissions: { policyType: "allMembers", policySet: {} },
  }),
}));
vi.mock("@/contexts/ConversationContext", () => ({
  useConversationContext: () => ({
    conversationId: "group-1",
    setReplyTarget: mocks.setReplyTarget,
  }),
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => ({
    inboxId: "c".repeat(64),
    conversations: { reactToMessage: mocks.react },
  }),
}));
vi.mock("react-router", () => ({
  useNavigate: () => mocks.navigate,
  useOutletContext: () => ({ conversationId: "group-1" }),
}));
vi.mock("@/hooks/useCollapsedMediaQuery", () => ({
  useCollapsedMediaQuery: () => false,
}));
vi.mock("@/components/Modal", () => ({
  Modal: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/layouts/ContentLayout", () => ({
  ContentLayout: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
}));
vi.mock("./MessageContentWithWrapper", () => ({
  MessageContentWithWrapper: () => null,
}));
vi.mock("./MessageContent", () => ({ MessageContent: () => null }));
const peer = "a".repeat(64);
const other = "b".repeat(64);
const member = (inboxId = peer, handle = "nick#2341"): Member => ({
  inboxId,
  identities: [],
  permissionLevel: "member",
  consentState: "unknown",
  handle,
  otherHandles: ["oldnick"],
});
const messageData = {
  id: "message-1",
  conversationId: "group-1",
  senderInboxId: peer,
  kind: "application",
  deliveryStatus: "published",
  sentAt: { ns: 1n },
  content: { kind: "text", value: "hello" },
  reactions: [],
};
const message = messageData as unknown as XmtpMessage;
const open = (children: ReactNode) =>
  render(<MantineProvider>{children}</MantineProvider>);
beforeEach(() => {
  vi.clearAllMocks();
  mocks.members = new Map([
    [peer, member()],
    [other, member(other, "bob#1234")],
  ]);
  mocks.messages = [
    { ...messageData, id: "parent-1" } as unknown as XmtpMessage,
  ];
});
afterEach(cleanup);
it("shows the current sender handle in message details and keeps the canonical inbox ID", () => {
  const view = open(<MessageProperties message={message} />);
  expect(screen.getByText("nick#2341")).toBeTruthy();
  expect(screen.getByText(peer)).toBeTruthy();
  mocks.members = new Map([[peer, member(peer, "newnick#2341")]]);
  view.rerender(
    <MantineProvider>
      <MessageProperties message={message} />
    </MantineProvider>,
  );
  expect(screen.getByText("newnick#2341")).toBeTruthy();
  expect(screen.queryByText("nick#2341")).toBeNull();
});
it("shows the creator's current handle above the copied inbox ID in conversation details", () => {
  open(<ManageDetailsModal />);
  expect(screen.getByText("nick#2341")).toBeTruthy();
  expect(screen.getByText(peer)).toBeTruthy();
});
it("names a replied-to author by its current handle while navigation uses the message ID", () => {
  open(
    <ReplyContent
      align="left"
      reply={{
        kind: "reply",
        referenceId: "parent-1",
        body: { kind: "text", value: "reply" },
      }}
      scrollToMessage={mocks.scroll}
    />,
  );
  expect(screen.getByText("nick#2341")).toBeTruthy();
  fireEvent.click(screen.getByText("message"));
  expect(mocks.scroll).toHaveBeenCalledExactlyOnceWith("parent-1");
});
const reaction = (
  senderInboxId: string,
  action: "added" | "removed" = "added",
) => ({
  senderInboxId,
  reaction: { schema: "unicode", action, content: "👍" },
});
it("names active reactors by current handles and omits a reactor after removal", () => {
  const reacted = {
    ...messageData,
    reactions: [reaction(peer), reaction(other), reaction(peer, "removed")],
  } as unknown as XmtpMessage;
  open(<Message message={reacted} scrollToMessage={mocks.scroll} />);
  const badge = screen.getByLabelText("👍: bob#1234");
  expect(screen.queryByLabelText(/nick#2341/)).toBeNull();
  fireEvent.click(badge);
  expect(mocks.react).toHaveBeenCalledWith(
    "message-1",
    { action: "added", schema: "unicode", content: "👍" },
    { shouldPush: true },
  );
});
it("refreshes reactor handles from member state and falls back to an inbox ID when no handle is known", () => {
  const reacted = {
    ...messageData,
    reactions: [reaction(peer)],
  } as unknown as XmtpMessage;
  const view = open(
    <Message message={reacted} scrollToMessage={mocks.scroll} />,
  );
  expect(screen.getByLabelText("👍: nick#2341")).toBeTruthy();
  mocks.members = new Map([[peer, member(peer, "newnick#2341")]]);
  view.rerender(
    <MantineProvider>
      <Message message={reacted} scrollToMessage={mocks.scroll} />
    </MantineProvider>,
  );
  expect(screen.getByLabelText("👍: newnick#2341")).toBeTruthy();
  mocks.members = new Map();
  view.rerender(
    <MantineProvider>
      <Message message={reacted} scrollToMessage={mocks.scroll} />
    </MantineProvider>,
  );
  expect(screen.getByLabelText("👍: aaaaaa...aaaa")).toBeTruthy();
});

const replyInsideAnotherAuthor = () => (
  <MemberPopover
    address=""
    avatar={null}
    conversationId="group-1"
    description={null}
    displayName={other}
    handle="bob#1234"
    inboxId={other}
    otherHandles={["oldbob"]}
    permissionLevel="member">
    <div>
      <ReplyContent
        align="left"
        reply={{
          kind: "reply",
          referenceId: "parent-1",
          body: { kind: "text", value: "reply" },
        }}
        scrollToMessage={mocks.scroll}
      />
    </div>
  </MemberPopover>
);
it("opens the replied-to author's profile instead of an enclosing author's profile", async () => {
  open(replyInsideAnotherAuthor());
  fireEvent.click(screen.getByText("nick#2341"));
  expect(await screen.findByText("aaaaaaaaaa...aaaaaaaa")).toBeTruthy();
  expect(screen.getByText("Other handles: oldnick")).toBeTruthy();
  expect(screen.queryByText("bbbbbbbbbb...bbbbbbbb")).toBeNull();
  expect(mocks.navigate).not.toHaveBeenCalled();
});
it("renders an unavailable reply author as a static inbox label", () => {
  mocks.members.delete(peer);
  open(replyInsideAnotherAuthor());
  const label = screen.getByText(peer);
  expect(label.closest("[tabindex='0']")).toBeNull();
  fireEvent.click(label);
  expect(screen.queryByText("bbbbbbbbbb...bbbbbbbb")).toBeNull();
});
