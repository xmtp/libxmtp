import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createNameResolver } from "@/user/NameResolver";

const mockFetch = vi.fn();
const originalFetch = global.fetch;
const ADDRESS_A = "0x1234567890123456789012345678901234567890";
const ADDRESS_B = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
const response = (address: unknown) => ({
  ok: true,
  json: async () => [{ address }],
});

describe("NameResolver", () => {
  beforeEach(() => {
    global.fetch = mockFetch;
    mockFetch.mockReset();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(0);
  });

  afterEach(() => {
    global.fetch = originalFetch;
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe("caching behavior", () => {
    it("should return address immediately without API call for valid addresses", async () => {
      const nameResolver = createNameResolver();
      const validAddress = ADDRESS_A;

      const result = await nameResolver(validAddress);
      expect(result).toBe(validAddress);
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it("caches names for ten minutes after fetch completion without extending on hits", async () => {
      mockFetch
        .mockImplementationOnce(async () => {
          vi.setSystemTime(1_000);
          return response(ADDRESS_A);
        })
        .mockResolvedValue(response(ADDRESS_B));
      const nameResolver = createNameResolver();
      const name = "fixed-expiry.eth";
      expect(await nameResolver(name)).toBe(ADDRESS_A);
      const filledAt = Date.now();
      vi.setSystemTime(filledAt + 599_999);
      expect(await nameResolver(name)).toBe(ADDRESS_A);
      expect(mockFetch).toHaveBeenCalledTimes(1);
      vi.setSystemTime(filledAt + 600_000);
      expect(await nameResolver(name)).toBe(ADDRESS_B);
      expect(mockFetch).toHaveBeenCalledTimes(2);
      expect(await nameResolver(name)).toBe(ADDRESS_B);
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it.each([
      "not-an-address",
      "0x1234",
      "0x52908400098527886E0F7030069857D2E4169Ee7",
      123,
    ])("rejects invalid answer %s without caching it", async (address) => {
      mockFetch
        .mockResolvedValueOnce(response(address))
        .mockResolvedValue(response(ADDRESS_B));
      const nameResolver = createNameResolver();
      const name = `invalid-${address}.eth`;
      expect(await nameResolver(name)).toBeNull();
      expect(await nameResolver(name)).toBe(ADDRESS_B);
      expect(await nameResolver(name)).toBe(ADDRESS_B);
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it.each([
      ["empty", { ok: true, json: async () => [] }],
      ["null", response(null)],
      ["empty-address", response("")],
      ["invalid", response("not-an-address")],
      [
        "http-error",
        { ok: false, status: 503, statusText: "Service Unavailable" },
      ],
    ])(
      "does not reuse expired addresses after %s replies and recovers",
      async (kind, reply) => {
        mockFetch
          .mockResolvedValueOnce(response(ADDRESS_A))
          .mockResolvedValueOnce(reply)
          .mockResolvedValue(response(ADDRESS_B));
        const nameResolver = createNameResolver();
        const name = `expired-${kind}.eth`;
        expect(await nameResolver(name)).toBe(ADDRESS_A);
        vi.setSystemTime(600_000);
        if (kind === "http-error") {
          await expect(nameResolver(name)).rejects.toThrow(
            "Service Unavailable (503)",
          );
        } else {
          expect(await nameResolver(name)).toBeNull();
        }
        expect(await nameResolver(name)).toBe(ADDRESS_B);
        expect(await nameResolver(name)).toBe(ADDRESS_B);
        expect(mockFetch).toHaveBeenCalledTimes(3);
      },
    );
  });
});
