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
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { HandleSection } from "./HandleSection";

const mocks = vi.hoisted(() => {
  class HandleTaken extends Error {}
  class HandleNotAllowed extends Error {}
  class HandleTooSoon extends Error {}
  class HandleLimitReached extends Error {}
  class InvalidArgument extends Error {}
  return {
    errors: {
      HandleTaken,
      HandleNotAllowed,
      HandleTooSoon,
      HandleLimitReached,
      InvalidArgument,
    },
    availableHandles: vi.fn<(name: string) => Promise<string[]>>(),
    claimHandle: vi.fn<(handle: string) => Promise<void>>(),
    inboxState:
      vi.fn<
        (
          refresh: boolean,
        ) => Promise<{ handle?: string; otherHandles: string[] }>
      >(),
    refresh: vi.fn(
      async (
        _refreshFromNetwork?: boolean,
      ): Promise<
        { handle: string | null; otherHandles: string[] } | undefined
      > => undefined,
    ),
    minNameLength: 3,
    handle: null as string | null,
    otherHandles: [] as string[],
    loaded: true,
    failed: false,
  };
});
vi.mock("@xmtp/browser-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof SDK>()),
  XmtpError: mocks.errors,
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => ({
    availableHandles: mocks.availableHandles,
    claimHandle: mocks.claimHandle,
    inboxState: mocks.inboxState,
    serverConfiguration: { handles: { minNameLength: mocks.minNameLength } },
  }),
}));
vi.mock("@/hooks/useOwnHandles", () => ({
  useOwnHandles: () => ({
    handle: mocks.handle,
    otherHandles: mocks.otherHandles,
    loaded: mocks.loaded,
    failed: mocks.failed,
    refresh: mocks.refresh,
  }),
}));
vi.mock("@/components/Modal", () => ({
  Modal: ({ children, opened }: { children: ReactNode; opened: boolean }) =>
    opened ? <dialog open>{children}</dialog> : null,
}));

const renderSection = () =>
  render(
    <MantineProvider>
      <HandleSection />
    </MantineProvider>,
  );

const button = (name: string) => screen.getByRole("button", { name });

const changeTo = async (handle: string) => {
  fireEvent.click(button("Change handle"));
  fireEvent.change(screen.getByRole("textbox", { name: "Handle" }), {
    target: { value: handle },
  });
  fireEvent.click(button("Continue"));
  fireEvent.click(
    await screen.findByRole("button", { name: `Change to ${handle}` }),
  );
  fireEvent.click(button("Change"));
};

beforeEach(() => {
  vi.clearAllMocks();
  // drop answers that a test queued and did not use
  mocks.availableHandles.mockReset();
  mocks.minNameLength = 3;
  mocks.handle = "nick#2341";
  mocks.otherHandles = ["nick#0912", "nickm"];
  mocks.loaded = true;
  mocks.failed = false;
});

afterEach(() => {
  cleanup();
});

describe("HandleSection", () => {
  it("shows a load error with Retry, and offers no claim, when the handles are unknown", async () => {
    mocks.loaded = false;
    mocks.failed = true;
    mocks.handle = null;
    mocks.otherHandles = [];
    mocks.refresh.mockImplementationOnce(async () => {
      mocks.loaded = true;
      mocks.failed = false;
      mocks.handle = "nick#2341";
      return { handle: "nick#2341", otherHandles: [] };
    });
    renderSection();
    expect(
      screen.getByText("Cannot load your handle. Try again."),
    ).toBeTruthy();
    expect(screen.queryByText("Choose a handle")).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Handle" })).toBeNull();

    fireEvent.click(button("Retry"));
    await screen.findByText("Your handle on this server");
    expect(mocks.refresh).toHaveBeenCalledExactlyOnceWith(true);
    expect(mocks.claimHandle).not.toHaveBeenCalled();
  });

  it("shows the current handle first and the other handles after it", () => {
    renderSection();
    expect(screen.getByText("Your handle on this server")).toBeTruthy();
    expect(screen.getByText("nick#0912 · nickm")).toBeTruthy();
  });

  it("offers a handle to a user who has none, and loads the new one", async () => {
    mocks.handle = null;
    mocks.otherHandles = [];
    mocks.availableHandles.mockResolvedValueOnce(["nick"]);
    mocks.claimHandle.mockResolvedValueOnce();
    renderSection();
    expect(screen.getByText("Choose a handle")).toBeTruthy();
    fireEvent.change(screen.getByRole("textbox", { name: "Handle" }), {
      target: { value: "nick" },
    });
    fireEvent.click(button("Continue"));
    fireEvent.click(await screen.findByRole("button", { name: "Claim nick" }));
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalled());
    expect(mocks.claimHandle).toHaveBeenCalledWith("nick");
  });

  // A transport error can arrive after the backend stored the claim.
  it("treats a failed claim that landed as a success, with no retry", async () => {
    mocks.availableHandles.mockResolvedValueOnce(["nicky"]);
    mocks.claimHandle.mockRejectedValueOnce(new Error("transport closed"));
    mocks.inboxState.mockResolvedValueOnce({
      handle: "nicky",
      otherHandles: ["nick#2341", "nick#0912", "nickm"],
    });
    renderSection();
    await changeTo("nicky");
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalled());
    expect(mocks.inboxState).toHaveBeenCalledExactlyOnceWith(true);
    expect(mocks.claimHandle).toHaveBeenCalledOnce();
    expect(screen.queryByText("Cannot claim nicky. Try again.")).toBeNull();
    // the editor closed, as after a claim that answered
    expect(screen.queryByRole("textbox", { name: "Handle" })).toBeNull();
  });

  it("shows the error when a failed claim did not land", async () => {
    mocks.availableHandles.mockResolvedValueOnce(["nicky"]);
    mocks.claimHandle.mockRejectedValueOnce(new Error("transport closed"));
    mocks.inboxState.mockResolvedValueOnce({
      handle: "nick#2341",
      otherHandles: ["nick#0912", "nickm"],
    });
    renderSection();
    await changeTo("nicky");
    await screen.findByText("Cannot claim nicky. Try again.");
    expect(mocks.inboxState).toHaveBeenCalledExactlyOnceWith(true);
    expect(mocks.refresh).not.toHaveBeenCalled();
  });

  // a first claim cannot be too soon: another installation claimed
  const claimFirst = async (
    refusal: Error = new mocks.errors.HandleTooSoon(),
  ) => {
    mocks.handle = null;
    mocks.otherHandles = [];
    mocks.availableHandles.mockResolvedValueOnce(["nick"]);
    mocks.claimHandle.mockRejectedValueOnce(refusal);
    const view = renderSection();
    fireEvent.change(screen.getByRole("textbox", { name: "Handle" }), {
      target: { value: "nick" },
    });
    fireEvent.click(button("Continue"));
    fireEvent.click(await screen.findByRole("button", { name: "Claim nick" }));
    return view;
  };

  it("reads the inbox again after a too-soon first claim, and blocks the editor", async () => {
    await claimFirst();
    await screen.findByText(
      "You changed your handle less than 7 days ago. Try again later.",
    );
    expect(mocks.refresh).toHaveBeenCalledExactlyOnceWith(true);
    expect(screen.getByText("Choose a handle")).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "Handle" })).toHaveProperty(
      "disabled",
      true,
    );
  });

  it("shows the handle that the network read finds after a too-soon first claim", async () => {
    mocks.refresh.mockImplementationOnce(async () => {
      mocks.handle = "nick#0412";
    });
    await claimFirst();
    await screen.findByText("Your handle on this server");
    expect(mocks.refresh).toHaveBeenCalledExactlyOnceWith(true);
    expect(screen.queryByText("Choose a handle")).toBeNull();
  });

  it("hides the editor after a first claim over the limit", async () => {
    await claimFirst(new mocks.errors.HandleLimitReached());
    await screen.findByText(
      "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.",
    );
    expect(mocks.refresh).toHaveBeenCalledExactlyOnceWith(true);
    expect(screen.getByText("Choose a handle")).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "Handle" })).toBeNull();
  });

  // the claim landed, its transport error hid that, and the re-read failed
  it("treats a too-soon retry of a claim that landed as a success", async () => {
    mocks.refresh.mockImplementationOnce(async () => {
      mocks.handle = "nick";
      return { handle: "nick", otherHandles: [] };
    });
    const view = await claimFirst();
    // the success path reads the handles again
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(2));
    expect(mocks.refresh.mock.calls[0]).toEqual([true]);
    // the mocked hook does not subscribe to the store, so render again
    view.rerender(
      <MantineProvider>
        <HandleSection />
      </MantineProvider>,
    );
    await screen.findByText("Your handle on this server");
    expect(
      screen.queryByText(
        "You changed your handle less than 7 days ago. Try again later.",
      ),
    ).toBeNull();
  });

  it("shows when the handle changed less than 7 days ago", async () => {
    mocks.availableHandles.mockResolvedValueOnce(["nicky"]);
    mocks.claimHandle.mockRejectedValueOnce(new mocks.errors.HandleTooSoon());
    renderSection();
    await changeTo("nicky");
    await screen.findByText(
      "You changed your handle less than 7 days ago. Try again later.",
    );
    expect(screen.getByRole("textbox", { name: "Handle" })).toHaveProperty(
      "disabled",
      true,
    );
    expect(button("Use nickm")).toHaveProperty("disabled", true);
  });

  it("waits for a new pick when someone claims the chosen handle first", async () => {
    mocks.availableHandles
      .mockResolvedValueOnce(["nicky", "nicky#0001", "nicky#0002"])
      // the fresh answer after the claim: nicky#0001 is taken now
      .mockResolvedValueOnce(["nicky#0002", "nicky#0003"]);
    mocks.claimHandle.mockRejectedValueOnce(new mocks.errors.HandleTaken());
    renderSection();
    await changeTo("nicky");
    await screen.findByText(
      "Someone claimed nicky a moment before you. Pick another handle.",
    );
    const radios = screen
      .getAllByRole<HTMLInputElement>("radio")
      .map((radio) => ({ value: radio.value, checked: radio.checked }));
    expect(radios).toEqual([
      { value: "nicky#0002", checked: true },
      { value: "nicky#0003", checked: false },
    ]);
    // the app claims only what the user picked (PD9)
    expect(mocks.claimHandle).toHaveBeenCalledTimes(1);
    expect(mocks.refresh).not.toHaveBeenCalled();
  });

  it("explains a switch back that the current minimum refuses", async () => {
    mocks.minNameLength = 5;
    mocks.otherHandles = ["nick#0912", "nk"];
    mocks.claimHandle.mockRejectedValueOnce(
      new mocks.errors.HandleNotAllowed(),
    );
    renderSection();
    fireEvent.click(button("Change handle"));
    fireEvent.click(button("Use nk"));
    expect(mocks.claimHandle).not.toHaveBeenCalled();
    fireEvent.click(button("Change"));
    await screen.findByText(
      "nk is shorter than this server now allows. It still finds you, but you cannot switch back to it.",
    );
    // the backend decided: the app sent the claim (D34)
    expect(mocks.claimHandle).toHaveBeenCalledWith("nk");
  });

  it("stops offering changes when all 64 are used", async () => {
    mocks.availableHandles.mockResolvedValueOnce(["nicky"]);
    mocks.claimHandle.mockRejectedValueOnce(
      new mocks.errors.HandleLimitReached(),
    );
    renderSection();
    await changeTo("nicky");
    await screen.findByText(
      "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.",
    );
    expect(screen.queryByRole("textbox", { name: "Handle" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Use nickm" })).toBeNull();
  });
});
