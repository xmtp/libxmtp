import { MantineProvider } from "@mantine/core";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { MemberCard } from "./MemberCard";
import { MemberPopoverIdentity } from "./MemberPopover";

const address = `0x${"1".repeat(40)}`;
const inboxId = "a".repeat(64);

afterEach(() => {
  cleanup();
});

describe("member rendering", () => {
  it("names a member by its current handle, with the inbox ID secondary", () => {
    render(
      <MantineProvider>
        <MemberCard
          address={address}
          displayName={address}
          avatar={null}
          description={null}
          handle="nick#2341"
          inboxId={inboxId}
        />
      </MantineProvider>,
    );
    expect(screen.getByText("nick#2341")).toBeTruthy();
    expect(screen.getByText("aaaaaa...aaaa")).toBeTruthy();
    expect(screen.queryByText(address)).toBeNull();
  });

  it("names a member without a handle by its address", () => {
    render(
      <MantineProvider>
        <MemberCard
          address={address}
          displayName={address}
          avatar={null}
          description={null}
          handle={null}
          inboxId={inboxId}
        />
      </MantineProvider>,
    );
    expect(screen.getByText(address)).toBeTruthy();
    expect(screen.queryByText("aaaaaa...aaaa")).toBeNull();
  });

  it("shows the other handles as secondary text in the popover", () => {
    render(
      <MantineProvider>
        <MemberPopoverIdentity
          address={address}
          displayName={address}
          handle="nick#2341"
          inboxId={inboxId}
          otherHandles={["nick#0912", "nickm"]}
        />
      </MantineProvider>,
    );
    expect(screen.getByText("nick#2341")).toBeTruthy();
    expect(screen.getByText("Other handles: nick#0912 · nickm")).toBeTruthy();
    expect(screen.getByText("aaaaaaaaaa...aaaaaaaa")).toBeTruthy();
  });

  it("shows no handle text in the popover for a member without a handle", () => {
    render(
      <MantineProvider>
        <MemberPopoverIdentity
          address={address}
          displayName={address}
          handle={null}
          inboxId={inboxId}
          otherHandles={[]}
        />
      </MantineProvider>,
    );
    expect(screen.getByText(address)).toBeTruthy();
    expect(screen.queryByText(/Other handles/)).toBeNull();
  });
});

it("uses the inbox ID for a member with no handle, address, or display name", () => {
  render(
    <MantineProvider>
      <MemberCard
        address=""
        displayName={null}
        avatar={null}
        description={null}
        handle={null}
        inboxId={inboxId}
      />
    </MantineProvider>,
  );
  expect(screen.getByText("aaaaaa...aaaa")).toBeTruthy();
});
