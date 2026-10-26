import { MantineProvider } from "@mantine/core";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import type { WalletSendCalls } from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { WalletSendCallsContent } from "./WalletSendCallsContent";

const mocks = vi.hoisted(() => ({
  sendTransactionAsync: vi.fn(),
  switchChainAsync: vi.fn(),
}));

vi.mock("wagmi", () => ({
  useChainId: () => 1,
  useSendTransaction: () => ({
    sendTransactionAsync: mocks.sendTransactionAsync,
  }),
  useSwitchChain: () => ({ switchChainAsync: mocks.switchChainAsync }),
}));
const requestedFrom = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const recipient = "0xcccccccccccccccccccccccccccccccccccccccc";
const token = "0xdddddddddddddddddddddddddddddddddddddddd";

const renderRequest = (chainId: string, calls: WalletSendCalls["calls"]) => {
  const request: WalletSendCalls = {
    version: "1.0",
    chainId,
    from: requestedFrom,
    calls,
  };
  render(
    <MantineProvider>
      <WalletSendCallsContent content={request} />
    </MantineProvider>,
  );
};

const expectReadOnly = async () => {
  await act(async () => {
    for (const button of screen.queryAllByRole("button")) {
      fireEvent.click(button);
    }
    for (const item of screen.getAllByRole("listitem")) {
      fireEvent.click(item);
    }
    await Promise.resolve();
  });
  expect(mocks.switchChainAsync).not.toHaveBeenCalled();
  expect(mocks.sendTransactionAsync).not.toHaveBeenCalled();
  expect(screen.queryByRole("button")).toBeNull();
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("peer-supplied wallet calls", () => {
  it("shows a native transfer's raw fields without sending it to the wallet", async () => {
    const value = "0xde0b6b3a7640000";
    const gas = "0x0000000000005208";
    renderRequest("0x2105", [
      {
        to: recipient,
        value,
        gas,
        metadata: {
          description: "Verify your access",
          transactionType: "verification",
          extra: new Map(),
        },
      },
    ]);

    expect(screen.getByText(recipient)).toBeVisible();
    expect(screen.getByText("0x2105")).toBeVisible();
    expect(screen.getByText(requestedFrom)).toBeVisible();
    expect(screen.getByText(value)).toBeVisible();
    expect(screen.getByText(gas)).toBeVisible();
    expect(screen.getByText("none")).toBeVisible();
    expect(screen.getByText("Description from sender")).toBeVisible();
    expect(screen.getByText("Verify your access")).toBeVisible();
    await expectReadOnly();
  });

  it("shows an ERC-20 approval's calldata without sending it to the wallet", async () => {
    const approval = `0x095ea7b3${recipient.slice(2).padStart(64, "0")}${"f".repeat(64)}`;
    renderRequest("0x1", [
      {
        to: token,
        data: approval,
        value: "0x0",
        metadata: {
          description: "Refresh token connection",
          transactionType: "verification",
          extra: new Map(),
        },
      },
    ]);

    expect(screen.getByText(token)).toBeVisible();
    expect(screen.getByText(approval)).toBeVisible();
    expect(screen.getByText("0x1")).toBeVisible();
    expect(screen.getByText(requestedFrom)).toBeVisible();
    expect(screen.getByText("0x0")).toBeVisible();
    expect(screen.getByText("none")).toBeVisible();
    expect(screen.getByText("Description from sender")).toBeVisible();
    expect(screen.getByText("Refresh token connection")).toBeVisible();
    await expectReadOnly();
  });
});
