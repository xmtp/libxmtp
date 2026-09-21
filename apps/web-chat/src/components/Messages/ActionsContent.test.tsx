import { act, fireEvent, render, screen } from "@testing-library/react";
import { MantineProvider } from "@mantine/core";
import type { Actions } from "@xmtp/browser-sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConversationProvider } from "@/contexts/ConversationContext";
import { dateToNs } from "@/helpers/date";
import { ActionsContent } from "./ActionsContent";

const mocks = vi.hoisted(() => ({ sendIntent: vi.fn() }));

vi.mock("@/hooks/useConversation", () => ({
  useConversation: () => ({ sendIntent: mocks.sendIntent }),
}));

const renderActions = (content: Actions) =>
  render(
    <MantineProvider>
      <ConversationProvider conversationId="conversation-id">
        <ActionsContent content={content} />
      </ConversationProvider>
    </MantineProvider>,
  );

describe("ActionsContent", () => {
  beforeEach(() => {
    mocks.sendIntent.mockReset();
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("disables actions when their effective expiration passes and does not send intents", async () => {
    const now = Date.now();
    renderActions({
      id: "actions-1",
      description: "Choose an action",
      expiresAtNs: dateToNs(new Date(now + 1_000)),
      actions: [
        { id: "group-expiry", label: "Group expiry" },
        {
          id: "action-expiry",
          label: "Action expiry",
          expiresAtNs: dateToNs(new Date(now + 2_000)),
        },
      ],
    });

    const groupExpiry = screen.getByRole("button", { name: "Group expiry" });
    const actionExpiry = screen.getByRole("button", {
      name: "Action expiry",
    });
    expect(groupExpiry).toBeEnabled();
    expect(actionExpiry).toBeEnabled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_001);
    });

    expect(groupExpiry).toBeDisabled();
    expect(actionExpiry).toBeEnabled();
    fireEvent.click(groupExpiry);
    fireEvent.click(actionExpiry);
    expect(mocks.sendIntent).toHaveBeenCalledTimes(1);
    expect(mocks.sendIntent).toHaveBeenCalledWith({
      id: "actions-1",
      actionId: "action-expiry",
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    expect(actionExpiry).toBeDisabled();
    fireEvent.click(actionExpiry);
    expect(mocks.sendIntent).toHaveBeenCalledTimes(1);
  });
});
