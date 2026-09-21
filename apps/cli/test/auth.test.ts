import { createServer, type ServerHttp2Stream } from "node:http2";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it, vi } from "vitest";
import { loadConfig, mergeConfig } from "../src/utils/config.js";
import { runCommand } from "./helpers.js";

afterEach(() => vi.unstubAllEnvs());

describe("API key authentication", () => {
  it("loads the key from the environment and preserves it when merging flags", () => {
    vi.stubEnv("XMTP_API_KEY", "test-key");
    const config = mergeConfig(loadConfig(), {
      backendUrl: "https://example.com",
    });
    expect(config.apiKey).toBe("test-key");
  });

  it.each(["", "wrong-key", "test-key"])(
    "sends credentials for static network commands: %s",
    async (apiKey) => {
      const seen: Array<string | undefined> = [];
      const server = createServer();
      server.on("stream", (stream: ServerHttp2Stream, headers) => {
        seen.push(headers.authorization);
        stream.on("error", () => {});
        const chunks: Buffer[] = [];
        stream.on("data", (chunk: Buffer) => chunks.push(chunk));
        stream.on("end", () => {
          stream.respond({
            ":status": 200,
            "content-type": "application/grpc",
            "grpc-status":
              headers.authorization === "Bearer test-key" ? "0" : "16",
          });
          // GetInboxIds responses echo the request identifiers, without inbox IDs.
          stream.end(Buffer.concat(chunks));
        });
      });
      await new Promise<void>((resolve) =>
        server.listen(0, "127.0.0.1", resolve),
      );
      const { port } = server.address() as AddressInfo;
      try {
        const result = await runCommand(
          [
            "can-message",
            "0x0000000000000000000000000000000000000001",
            "--backend-url",
            `http://127.0.0.1:${port}`,
            "--json",
          ],
          { env: { XMTP_API_KEY: apiKey } },
        );
        expect(seen.length).toBeGreaterThan(0);
        expect(
          seen.every(
            (value) => value === (apiKey ? `Bearer ${apiKey}` : undefined),
          ),
        ).toBe(true);
        expect(result.exitCode === 0, result.stderr).toBe(
          apiKey === "test-key",
        );
        if (apiKey) {
          expect(result.stdout).not.toContain(apiKey);
          expect(result.stderr).not.toContain(apiKey);
        }
      } finally {
        await new Promise<void>((resolve) => server.close(() => resolve()));
      }
    },
  );
});
