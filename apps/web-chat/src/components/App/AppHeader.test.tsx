import { MantineProvider } from "@mantine/core";
import { cleanup, render, screen } from "@testing-library/react";
import type { Client } from "@xmtp/browser-sdk";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AppHeader } from "./AppHeader";

const mocks = vi.hoisted(() => ({ handle: null as string | null }));
vi.mock("@/hooks/useOwnHandles", () => ({
  useOwnHandles: () => ({
    handle: mocks.handle,
    otherHandles: [],
    loaded: true,
    refresh: async () => {},
  }),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ backendUrl: "http://127.0.0.1:5150" }),
}));
vi.mock("@/components/App/AppMenu", () => ({ AppMenu: () => null }));

const address = `0x${"1".repeat(40)}`;

const accountButton = () => {
  const client = { identity: { identifier: address, kind: "ethereum" } };
  render(
    <MantineProvider>
      <MemoryRouter>
        <AppHeader client={client as unknown as Client} />
      </MemoryRouter>
    </MantineProvider>,
  );
  return screen.getByRole("button", { name: address });
};

afterEach(() => {
  cleanup();
});

describe("AppHeader", () => {
  it("shows the own handle on the account button", () => {
    mocks.handle = "nick#2341";
    expect(accountButton().textContent).toBe("nick#2341");
  });

  it("shows the short address when the inbox has no handle", () => {
    mocks.handle = null;
    expect(accountButton().textContent).toBe("0x1111...1111");
  });
});
