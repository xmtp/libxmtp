import { MantineProvider } from "@mantine/core";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { SwitchBackendModal } from "./SwitchBackendModal";

const mocks = vi.hoisted(() => ({
  disconnect: vi.fn(),
  setBackendUrl: vi.fn(),
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useXMTP: () => ({ disconnect: mocks.disconnect }),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({
    backendUrl: "https://old.example",
    setBackendUrl: mocks.setBackendUrl,
  }),
}));
vi.mock("@/components/Modal", () => ({
  Modal: ({
    children,
    onClose,
    opened,
  }: {
    children: ReactNode;
    onClose: () => void;
    opened: boolean;
  }) =>
    opened ? (
      <div>
        <button onClick={onClose}>Close overlay</button>
        {children}
      </div>
    ) : null,
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("keeps the switch dialog open while disconnect is pending", async () => {
  const held = Promise.withResolvers<void>();
  mocks.disconnect.mockReturnValueOnce(held.promise);
  render(
    <MantineProvider>
      <MemoryRouter initialEntries={["/?backend=https%3A%2F%2Fnext.example"]}>
        <SwitchBackendModal />
      </MemoryRouter>
    </MantineProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Switch backend" }));
  expect(mocks.disconnect).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "Close overlay" }));
  const dialogStillOpen = screen.queryByRole("button", {
    name: "Close overlay",
  });
  expect(dialogStillOpen).not.toBeNull();
  expect(mocks.setBackendUrl).not.toHaveBeenCalled();
  await act(async () => {
    held.resolve();
    await held.promise;
  });
  expect(mocks.setBackendUrl).toHaveBeenCalledWith("https://next.example");
});
