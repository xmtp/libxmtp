import { MantineProvider } from "@mantine/core";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ChooseHandlePrompt } from "./ChooseHandlePrompt";

const mocks = vi.hoisted(() => ({
  inboxId: "",
  handle: null as string | null,
  loaded: true,
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => ({ inboxId: mocks.inboxId }),
}));
vi.mock("@/hooks/useOwnHandles", () => ({
  useOwnHandles: () => ({
    handle: mocks.handle,
    otherHandles: [],
    loaded: mocks.loaded,
    refresh: async () => {},
  }),
}));

const inboxA = "a".repeat(64);
const inboxB = "b".repeat(64);
const title = "Choose a handle";

const show = (inboxId: string) => {
  mocks.inboxId = inboxId;
  return render(
    <MantineProvider>
      <MemoryRouter>
        <Routes>
          <Route path="/" element={<ChooseHandlePrompt />} />
          <Route path="/identity" element={<p>identity page</p>} />
        </Routes>
      </MemoryRouter>
    </MantineProvider>,
  );
};

beforeEach(() => {
  mocks.handle = null;
  mocks.loaded = true;
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("ChooseHandlePrompt", () => {
  it("asks an inbox with no handle to choose one, and opens the identity page", async () => {
    show(inboxA);
    fireEvent.click(screen.getByText(title));
    await screen.findByText("identity page");
  });

  it("hides after dismissal, and keeps the dismissal for that inbox only", () => {
    const first = show(inboxA);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText(title)).toBeNull();
    expect(localStorage.getItem(`XMTP_HANDLE_PROMPT_DISMISSED_${inboxA}`)).toBe(
      "true",
    );
    first.unmount();

    // the same inbox later: still dismissed
    const again = show(inboxA);
    expect(screen.queryByText(title)).toBeNull();
    again.unmount();

    // another inbox in the same browser: not dismissed
    show(inboxB);
    expect(screen.getByText(title)).toBeTruthy();
  });

  it("does not show before the handles are loaded", () => {
    // the hook gives a null handle until the first read finishes
    mocks.loaded = false;
    show(inboxA);
    expect(screen.queryByText(title)).toBeNull();
  });

  it("does not show for an inbox that has a handle", () => {
    mocks.handle = "nick#2341";
    show(inboxA);
    expect(screen.queryByText(title)).toBeNull();
  });
});
