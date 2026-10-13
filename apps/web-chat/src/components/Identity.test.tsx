import { MantineProvider } from "@mantine/core";
import { cleanup, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Identity, type IdentityProps } from "./Identity";

vi.mock("@/components/Conversation/MemberPopover", () => ({
  MemberPopover: ({ children }: { children: ReactNode }) => children,
  useMemberPopover: () => ({ opened: false, setOpened: () => {} }),
}));

const address = `0x${"1".repeat(40)}`;

const renderSender = (
  props: Pick<IdentityProps, "displayName" | "handle" | "otherHandles">,
) =>
  render(
    <MantineProvider>
      <Identity
        address={address}
        avatar={null}
        conversationId="group"
        description={null}
        inboxId={"a".repeat(64)}
        permissionLevel="member"
        {...props}
      />
    </MantineProvider>,
  );

afterEach(() => {
  cleanup();
});

describe("message sender badge", () => {
  it("names the sender by its current handle", () => {
    renderSender({
      displayName: address,
      handle: "nick#2341",
      otherHandles: ["nick#0912"],
    });
    expect(screen.getByText("nick#2341")).toBeTruthy();
    expect(screen.queryByText(address)).toBeNull();
  });

  it("names a sender without a handle by its display name, else its address", () => {
    renderSender({ displayName: address, handle: null, otherHandles: [] });
    expect(screen.getByText(address)).toBeTruthy();
    cleanup();
    renderSender({ displayName: null, handle: null, otherHandles: [] });
    expect(screen.getByText("0x1111...1111")).toBeTruthy();
  });
});
