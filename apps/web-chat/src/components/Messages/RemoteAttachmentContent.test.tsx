import { MantineProvider } from "@mantine/core";
import {
  act,
  cleanup,
  fireEvent,
  render,
  waitFor,
} from "@testing-library/react";
import type { RemoteAttachment } from "@xmtp/browser-sdk";
import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type * as attachmentHelpers from "@/helpers/attachment";

import { RemoteAttachmentContent } from "./RemoteAttachmentContent";

const { download } = vi.hoisted(() => ({ download: vi.fn() }));

vi.mock("@/helpers/attachment", async (importOriginal) => ({
  ...(await importOriginal<typeof attachmentHelpers>()),
  downloadRemoteAttachment: download,
}));

const content = (name: string): RemoteAttachment => ({
  url: `https://example.com/${name}`,
  contentDigest: name,
  secret: new Uint8Array([1]),
  salt: new Uint8Array([2]),
  nonce: new Uint8Array([3]),
  scheme: "https",
  contentLength: 1,
  filename: `${name}.png`,
});

const attachment = {
  filename: "image.png",
  mimeType: "image/png",
  content: new Uint8Array([1]),
};

const view = (source: RemoteAttachment) => (
  <MantineProvider>
    <RemoteAttachmentContent content={source} align="left" />
  </MantineProvider>
);

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  download.mockReset();
});

describe("remote attachment lifecycle", () => {
  it("ignores an old download after the attachment changes", async () => {
    const oldDownload = Promise.withResolvers<typeof attachment>();
    const newDownload = Promise.withResolvers<typeof attachment>();
    download.mockReturnValueOnce(oldDownload.promise);
    download.mockReturnValueOnce(newDownload.promise);
    const createUrl = vi.spyOn(URL, "createObjectURL");
    const first = content("pending-first");
    const second = content("pending-second");
    const result = render(view(first));

    result.rerender(view(second));
    await act(async () => {
      oldDownload.resolve(attachment);
      await oldDownload.promise;
    });
    expect(createUrl).not.toHaveBeenCalled();
    expect(result.queryByRole("img")).toBeNull();

    await act(async () => {
      newDownload.resolve(attachment);
      await newDownload.promise;
    });
    expect(result.getByRole("img").getAttribute("alt")).toBe(second.filename);
    expect(createUrl).toHaveBeenCalledTimes(1);
  });

  it("removes the old URL immediately and revokes it when content changes", async () => {
    download.mockResolvedValueOnce(attachment);
    const nextDownload = Promise.withResolvers<typeof attachment>();
    download.mockReturnValueOnce(nextDownload.promise);
    const revokeUrl = vi.spyOn(URL, "revokeObjectURL");
    const result = render(view(content("loaded-first")));
    const image = await result.findByRole("img");
    const oldUrl = image.getAttribute("src");

    result.rerender(view(content("loaded-second")));
    expect(result.queryByRole("img")).toBeNull();
    expect(revokeUrl).toHaveBeenCalledWith(oldUrl);
    await act(async () => {
      nextDownload.resolve(attachment);
      await nextDownload.promise;
    });
    const newUrl = result.getByRole("img").getAttribute("src");
    expect(newUrl).not.toBe(oldUrl);
    result.unmount();
    expect(revokeUrl).toHaveBeenCalledWith(newUrl);
  });

  it("keeps the URL when an equivalent content object is rendered", async () => {
    download.mockResolvedValue(attachment);
    const revokeUrl = vi.spyOn(URL, "revokeObjectURL");
    const source = content("equivalent");
    const result = render(view(source));
    const image = await result.findByRole("img");
    const url = image.getAttribute("src");

    result.rerender(view({ ...source, secret: new Uint8Array(source.secret) }));
    expect(result.getByRole("img").getAttribute("src")).toBe(url);
    expect(download).toHaveBeenCalledTimes(1);
    expect(revokeUrl).not.toHaveBeenCalled();
  });

  it("retries a failed download", async () => {
    download.mockRejectedValueOnce(new Error("Download failed"));
    download.mockResolvedValueOnce(attachment);
    const result = render(view(content("retry")));
    const retry = await result.findByRole("button", { name: "Retry" });

    fireEvent.click(retry);
    await result.findByRole("img");
    expect(download).toHaveBeenCalledTimes(2);
    expect(result.queryByText("Unable to load attachment")).toBeNull();
  });

  it("shares downloads but gives each mount its own URL", async () => {
    download.mockResolvedValue(attachment);
    const revokeUrl = vi.spyOn(URL, "revokeObjectURL");
    const source = content("shared");
    const first = render(view(source));
    await waitFor(() => {
      expect(first.container.querySelector("img")).not.toBeNull();
    });
    const firstUrl = first.container.querySelector("img")?.getAttribute("src");
    const second = render(view(source));
    await waitFor(() => {
      expect(second.container.querySelector("img")).not.toBeNull();
    });
    const secondUrl = second.container
      .querySelector("img")
      ?.getAttribute("src");

    expect(download).toHaveBeenCalledTimes(1);
    expect(firstUrl).not.toBe(secondUrl);
    first.unmount();
    expect(revokeUrl).toHaveBeenCalledWith(firstUrl);
    expect(revokeUrl).not.toHaveBeenCalledWith(secondUrl);
    second.unmount();
    expect(revokeUrl).toHaveBeenCalledWith(secondUrl);
  });

  it("does not create a URL for a download completed after unmount", async () => {
    const pending = Promise.withResolvers<typeof attachment>();
    download.mockReturnValue(pending.promise);
    const createUrl = vi.spyOn(URL, "createObjectURL");
    const result = render(
      <StrictMode>{view(content("unmounted"))}</StrictMode>,
    );
    result.unmount();
    await act(async () => {
      pending.resolve(attachment);
      await pending.promise;
    });

    expect(download).toHaveBeenCalledTimes(1);
    expect(createUrl).not.toHaveBeenCalled();
  });
});
