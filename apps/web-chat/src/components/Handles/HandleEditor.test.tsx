import { MantineProvider } from "@mantine/core";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ClaimOutcome } from "@/helpers/handles";

import { HandleEditor, type HandleEditorProps } from "./HandleEditor";

const mocks = vi.hoisted(() => ({
  availableHandles: vi.fn<(name: string) => Promise<string[]>>(),
  minNameLength: 3,
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => ({
    availableHandles: mocks.availableHandles,
    serverConfiguration: { handles: { minNameLength: mocks.minNameLength } },
  }),
}));
vi.mock("@/components/Modal", () => ({
  Modal: ({ children, opened }: { children: ReactNode; opened: boolean }) =>
    opened ? <dialog open>{children}</dialog> : null,
}));

const SUGGESTIONS = [
  "nick#2341",
  "nick#0912",
  "nick#5530",
  "nick#6418",
  "nick#7001",
];

const renderEditor = (props: Partial<HandleEditorProps> = {}) => {
  const claim =
    props.claim ??
    vi.fn<(handle: string) => Promise<ClaimOutcome>>(async () => "claimed");
  render(
    <MantineProvider>
      <HandleEditor
        currentHandle={null}
        otherHandles={[]}
        {...props}
        claim={claim}
      />
    </MantineProvider>,
  );
  return { claim };
};

const type = (value: string) => {
  fireEvent.change(screen.getByRole("textbox", { name: "Handle" }), {
    target: { value },
  });
};

const button = (name: string) => screen.getByRole("button", { name });

const radios = () =>
  screen
    .getAllByRole<HTMLInputElement>("radio")
    .map((radio) => ({ value: radio.value, checked: radio.checked }));

beforeEach(() => {
  mocks.minNameLength = 3;
  mocks.availableHandles.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("HandleEditor", () => {
  it("checks the grammar and the published minimum as the user types", () => {
    mocks.minNameLength = 5;
    renderEditor();
    expect(screen.getByText(/^5 to 27 lowercase letters/)).toBeTruthy();
    type("nick");
    expect(
      screen.getByText("Use at least 5 characters on this server."),
    ).toBeTruthy();
    expect(button("Continue")).toHaveProperty("disabled", true);
    type("1nick");
    expect(screen.getByText("Start with a letter.")).toBeTruthy();
    type("nicholas_the_magnificent_one");
    expect(screen.getByText("28/27")).toBeTruthy();
    expect(mocks.availableHandles).not.toHaveBeenCalled();
  });

  it.each(["\u212Aate", "\u0130nci"])(
    "keeps a non-ASCII capital in %s, so the grammar refuses it",
    (input) => {
      renderEditor();
      // toLowerCase would fold U+212A KELVIN SIGN to "k" and admit "kate"
      type(input);
      expect(
        screen.getByRole<HTMLInputElement>("textbox", { name: "Handle" }).value,
      ).toBe(input);
      expect(
        screen.getByText("Use only lowercase letters, numbers, _ and -."),
      ).toBeTruthy();
      expect(button("Continue")).toHaveProperty("disabled", true);
    },
  );

  it("lowercases ASCII capitals as the user types", () => {
    renderEditor();
    type("Nick#2341");
    expect(
      screen.getByRole<HTMLInputElement>("textbox", { name: "Handle" }).value,
    ).toBe("nick#2341");
    expect(button("Continue")).toHaveProperty("disabled", false);
  });

  it("shows the check, then a free name", async () => {
    const pending = Promise.withResolvers<string[]>();
    mocks.availableHandles.mockReturnValueOnce(pending.promise);
    const { claim } = renderEditor();
    type("Nick");
    fireEvent.click(button("Continue"));
    expect(screen.getByText("Checking nick…")).toBeTruthy();
    expect(button("Continue")).toHaveProperty("disabled", true);
    await act(async () => pending.resolve(["nick", ...SUGGESTIONS]));
    expect(screen.getByText("nick is available.")).toBeTruthy();
    fireEvent.click(button("Claim nick"));
    await waitFor(() => expect(claim).toHaveBeenCalledWith("nick"));
    expect(mocks.availableHandles).toHaveBeenCalledWith("nick");
  });

  it("offers 3 suggestions, and 3 more on Show more", async () => {
    mocks.availableHandles.mockResolvedValueOnce(SUGGESTIONS);
    renderEditor();
    type("nick");
    fireEvent.click(button("Continue"));
    await screen.findByText("nick is taken. These handles are free:");
    expect(radios()).toEqual([
      { value: "nick#2341", checked: true },
      { value: "nick#0912", checked: false },
      { value: "nick#5530", checked: false },
    ]);
    expect(button("Claim nick#2341")).toBeTruthy();
    fireEvent.click(button("Show more"));
    expect(radios().map((radio) => radio.value)).toEqual(SUGGESTIONS);
    expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
  });

  it("pauses on a pick that someone claimed first, and shows fresh free handles", async () => {
    // nick#0912 was free in the first answer, and is taken by the second
    const fresh = ["nick#5530", "nick#6418", "nick#7001", "nick#8123"];
    mocks.availableHandles
      .mockResolvedValueOnce(SUGGESTIONS)
      .mockResolvedValueOnce(fresh);
    const claim = vi.fn<(handle: string) => Promise<ClaimOutcome>>(
      async () => "taken",
    );
    renderEditor({ claim });
    type("nick");
    fireEvent.click(button("Continue"));
    fireEvent.click(
      await screen.findByRole("button", { name: "Claim nick#2341" }),
    );
    await screen.findByText(
      "Someone claimed nick#2341 a moment before you. Pick another handle.",
    );
    expect(mocks.availableHandles.mock.calls).toEqual([["nick"], ["nick"]]);
    expect(radios()).toEqual([
      { value: "nick#5530", checked: true },
      { value: "nick#6418", checked: false },
      { value: "nick#7001", checked: false },
    ]);
    expect(button("Claim nick#5530")).toBeTruthy();
    fireEvent.click(button("Show more"));
    expect(radios().map((radio) => radio.value)).toEqual(fresh);
    // the app never claims a handle that the user did not choose (PD9)
    expect(claim).toHaveBeenCalledTimes(1);
  });

  it("offers a retry when the lookup after a taken pick fails", async () => {
    mocks.availableHandles
      .mockResolvedValueOnce(SUGGESTIONS)
      .mockRejectedValueOnce(new Error("offline"));
    const claim = vi.fn<(handle: string) => Promise<ClaimOutcome>>(
      async () => "taken",
    );
    renderEditor({ claim });
    type("nick");
    fireEvent.click(button("Continue"));
    fireEvent.click(
      await screen.findByRole("button", { name: "Claim nick#2341" }),
    );
    await screen.findByText("Cannot check this handle. Try again.");
    expect(screen.queryAllByRole("radio")).toEqual([]);
    expect(claim).toHaveBeenCalledTimes(1);
  });

  it("asks for another name when the server does not allow it", async () => {
    mocks.availableHandles.mockResolvedValueOnce(["admin"]);
    renderEditor({ claim: async () => "notAllowed" });
    type("admin");
    fireEvent.click(button("Continue"));
    fireEvent.click(await screen.findByRole("button", { name: "Claim admin" }));
    await screen.findByText("admin is not allowed on this server.");
    expect(screen.getByText("Choose a different name.")).toBeTruthy();
    expect(button("Continue")).toHaveProperty("disabled", true);
  });

  it("looks again when no free code was found", async () => {
    mocks.availableHandles.mockResolvedValue([]);
    renderEditor();
    type("nick");
    fireEvent.click(button("Continue"));
    await screen.findByText(
      "nick and the codes we checked are all taken. Look again, or try a different name.",
    );
    fireEvent.click(button("Look again"));
    await waitFor(() =>
      expect(mocks.availableHandles).toHaveBeenCalledTimes(2),
    );
  });

  it("offers a retry when the check fails", async () => {
    mocks.availableHandles
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(["nick"]);
    renderEditor();
    type("nick");
    fireEvent.click(button("Continue"));
    await screen.findByText("Cannot check this handle. Try again.");
    fireEvent.click(button("Retry"));
    await screen.findByText("nick is available.");
  });

  it("asks before it changes a current handle", async () => {
    mocks.availableHandles.mockResolvedValueOnce(["nicky"]);
    const { claim } = renderEditor({ currentHandle: "nick#2341" });
    type("nicky");
    fireEvent.click(button("Continue"));
    fireEvent.click(
      await screen.findByRole("button", { name: "Change to nicky" }),
    );
    expect(
      screen.getByText(
        "People will see nicky as your handle. nick#2341 stays yours, and people can still find you by it. You cannot change your handle again for 7 days.",
      ),
    ).toBeTruthy();
    fireEvent.click(button("Cancel"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(claim).not.toHaveBeenCalled();
    fireEvent.click(button("Change to nicky"));
    fireEvent.click(button("Change"));
    await waitFor(() => expect(claim).toHaveBeenCalledWith("nicky"));
  });

  it("refuses the current handle as the user types it", () => {
    renderEditor({ currentHandle: "nick#2341" });
    type("nick#2341");
    expect(screen.getByText("This is your current handle.")).toBeTruthy();
    expect(button("Continue")).toHaveProperty("disabled", true);
    expect(mocks.availableHandles).not.toHaveBeenCalled();
  });

  it("treats a typed other handle as a switch back, with no lookup", async () => {
    const { claim } = renderEditor({
      currentHandle: "nick#2341",
      otherHandles: ["nick#0912", "nickm"],
    });
    type("nickm");
    fireEvent.click(button("Continue"));
    await screen.findByText(
      "nickm is one of your handles. Switching back counts as a change.",
    );
    expect(mocks.availableHandles).not.toHaveBeenCalled();
    fireEvent.click(button("Change to nickm"));
    fireEvent.click(button("Change"));
    await waitFor(() => expect(claim).toHaveBeenCalledWith("nickm"));
  });
});
