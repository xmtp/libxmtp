import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";

// OPFS with Synchronous Access Handles (SAH) requires a Web Worker context.
// These tests run the OPFS functions inside a dedicated worker.

describe("OPFS File Management", () => {
  let worker: Worker;
  let messageId = 0;

  // Helper to call worker methods
  const callWorker = (method: string, params?: any) => {
    return new Promise((resolve, reject) => {
      const id = ++messageId;

      const handler = (event: MessageEvent) => {
        if (event.data.id === id) {
          worker.removeEventListener("message", handler);
          if (event.data.error) {
            reject(new Error(event.data.error));
          } else {
            resolve(event.data.result);
          }
        }
      };

      worker.addEventListener("message", handler);
      worker.postMessage({ id, method, params });
    });
  };

  beforeAll(async () => {
    // Create worker with module type
    worker = new Worker(new URL("./opfs.worker.js", import.meta.url), {
      type: "module",
    });
  });

  afterAll(() => {
    if (worker) {
      worker.terminate();
    }
  });

  beforeEach(async () => {
    // Clear all files before each test
    await callWorker("clearAll");
  });

  describe("Basic OPFS operations", () => {
    it("should list files (initially empty after clear)", async () => {
      const files = await callWorker("listFiles");
      expect(files).toHaveLength(0);
    });

    it("should report file count as 0 after clear", async () => {
      const count = await callWorker("fileCount");
      expect(typeof count).toBe("number");
      expect(count).toBe(0);
    });

    it("should report pool capacity", async () => {
      const capacity = await callWorker("poolCapacity");
      expect(typeof capacity).toBe("number");
      expect(capacity).toBeGreaterThanOrEqual(6); // Initial capacity is 6
    });

    it("should return false for non-existent file", async () => {
      const exists = await callWorker("fileExists", {
        filename: "non-existent-db",
      });
      expect(exists).toBe(false);
    });

    it("should return false when deleting non-existent file", async () => {
      const deleted = await callWorker("deleteFile", {
        filename: "non-existent-file",
      });
      expect(deleted).toBe(false);
    });
  });

  describe("OPFS with persistent client", () => {
    it("should create database file when client is created", async () => {
      const dbPath = "test-client-db";

      // Create a client with persistent storage
      const { inboxId } = (await callWorker("createClient", { dbPath })) as {
        inboxId: string;
      };
      expect(inboxId).toBeDefined();

      // Check that files were created
      const count = await callWorker("fileCount");
      expect(count).toBeGreaterThan(0);

      // The database should exist
      const exists = await callWorker("fileExists", { filename: dbPath });
      expect(exists).toBe(true);

      // List should include the file
      const files = await callWorker("listFiles");
      expect(files).toContain(dbPath);
      expect(files).toHaveLength(1);
    });

    it("should create multiple database files", async () => {
      const dbPath1 = "test-client-db-1";
      const dbPath2 = "test-client-db-2";

      await callWorker("createClient", { dbPath: dbPath1 });
      await callWorker("createClient", { dbPath: dbPath2 });

      const files = await callWorker("listFiles");
      expect(files).toContain(dbPath1);
      expect(files).toContain(dbPath2);
      expect(files).toHaveLength(2);

      expect(await callWorker("fileExists", { filename: dbPath1 })).toBe(true);
      expect(await callWorker("fileExists", { filename: dbPath2 })).toBe(true);
    });

    it("should delete a specific database file", async () => {
      const dbPath = "test-delete-db";

      await callWorker("createClient", { dbPath });
      expect(await callWorker("fileExists", { filename: dbPath })).toBe(true);

      // Delete the file
      const deleted = await callWorker("deleteFile", { filename: dbPath });
      expect(deleted).toBe(true);

      // Verify it's gone
      expect(await callWorker("fileExists", { filename: dbPath })).toBe(false);

      const files = await callWorker("listFiles");
      expect(files).toHaveLength(0);
    });

    it("should clear all database files", async () => {
      // Create multiple databases
      await callWorker("createClient", { dbPath: "clear-test-1" });
      await callWorker("createClient", { dbPath: "clear-test-2" });

      const countBefore = await callWorker("fileCount");
      expect(countBefore).toBeGreaterThan(0);

      // Clear all
      await callWorker("clearAll");

      const countAfter = await callWorker("fileCount");
      expect(countAfter).toBe(0);

      const files = await callWorker("listFiles");
      expect(files).toHaveLength(0);
    });
  });

  describe("Database export and import", () => {
    it("should export a database file", async () => {
      const dbPath = "test-export-db";

      // Create a client with persistent storage
      await callWorker("createClient", { dbPath });

      // Export the database
      const data = (await callWorker("exportDb", {
        filename: dbPath,
      })) as Uint8Array;

      // Verify we got data back
      expect(data instanceof Uint8Array).toBe(true);
      expect(data.length).toBeGreaterThan(0);

      // SQLite databases start with "SQLite format 3\0"
      const header = String.fromCharCode(...data.slice(0, 16));
      expect(header).toBe("SQLite format 3\0");
    });

    it("should import a database file", async () => {
      const originalDbPath = "test-import-original";
      const importDbPath = "test-import-new";

      // Create a client to generate a database
      await callWorker("createClient", { dbPath: originalDbPath });

      // Export the original database
      const exportedData = await callWorker("exportDb", {
        filename: originalDbPath,
      });

      // Import it with a new name
      await callWorker("importDb", {
        filename: importDbPath,
        data: exportedData,
      });

      // Verify the imported file exists
      expect(await callWorker("fileExists", { filename: importDbPath })).toBe(
        true,
      );

      // Verify we now have both files
      const files = await callWorker("listFiles");
      expect(files).toContain(originalDbPath);
      expect(files).toContain(importDbPath);
      expect(files).toHaveLength(2);
    });

    it("should replace database by deleting then importing", async () => {
      const dbPath = "test-replace-db";

      // Create first client
      const { inboxId: originalInboxId } = (await callWorker("createClient", {
        dbPath,
      })) as {
        inboxId: string;
      };

      // Export the original database
      const exportedData = (await callWorker("exportDb", {
        filename: dbPath,
      })) as Uint8Array;
      const originalSize = exportedData.length;

      // Delete and create a new client with same path (different data)
      await callWorker("deleteFile", { filename: dbPath });
      const { inboxId: client2InboxId } = (await callWorker("createClient", {
        dbPath,
      })) as {
        inboxId: string;
      };
      expect(client2InboxId).not.toBe(originalInboxId);

      // Delete the new database and import the original
      await callWorker("deleteFile", { filename: dbPath });
      await callWorker("importDb", {
        filename: dbPath,
        data: exportedData,
      });

      // Re-export and verify size matches original
      const restoredData = (await callWorker("exportDb", {
        filename: dbPath,
      })) as Uint8Array;
      expect(restoredData.length).toBe(originalSize);
    });

    it("should fail to export non-existent database", async () => {
      await expect(
        callWorker("exportDb", { filename: "non-existent-db" }),
      ).rejects.toThrow();
    });

    it("should fail to import invalid data", async () => {
      // Try to import garbage data (not a valid SQLite database)
      const invalidData = [1, 2, 3, 4, 5];

      await expect(
        callWorker("importDb", {
          filename: "invalid-import",
          data: invalidData,
        }),
      ).rejects.toThrow();
    });

    it("should roundtrip export and import", async () => {
      const originalDbPath = "test-roundtrip-original";
      const copyDbPath = "test-roundtrip-copy";

      // Create original database
      await callWorker("createClient", { dbPath: originalDbPath });

      // Export
      const exportedData = (await callWorker("exportDb", {
        filename: originalDbPath,
      })) as Uint8Array;

      // Import as copy
      await callWorker("importDb", {
        filename: copyDbPath,
        data: exportedData,
      });

      // Export the copy
      const reExportedData = (await callWorker("exportDb", {
        filename: copyDbPath,
      })) as Uint8Array;

      // Verify they match
      expect(reExportedData.length).toBe(exportedData.length);
      expect(reExportedData).toEqual(exportedData);
    });
  });
});
