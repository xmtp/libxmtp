import { spawn, type ChildProcess } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

async function freePort() {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  if (address === null || typeof address === "string") {
    throw new Error("The recovery backend has no TCP address");
  }
  return address.port;
}

async function within<T>(operation: Promise<T>, ms: number, label: string) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(label)), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

// This process uses the shared database but owns its ports and shutdown signal.
// No signal is sent to the shared backend or to an unrelated process.
export async function createRecoveryBackend() {
  const binary = process.env.XMTP_RECOVERY_BACKEND_BINARY;
  if (!binary || !process.env.DATABASE_URL) {
    throw new Error("Recovery backend needs a binary path and DATABASE_URL");
  }
  const port = await freePort();
  let metricsPort = await freePort();
  while (metricsPort === port) metricsPort = await freePort();
  const directory = await mkdtemp(join(tmpdir(), "xmtp-sdk-recovery-"));
  const config = join(directory, "backend.toml");
  let child: ChildProcess | undefined;
  let exited: Promise<void> | undefined;
  let spawnError: Error | undefined;
  let diagnostic = "";
  await writeFile(
    config,
    `[server]
identifier = "org.xmtp.local"
listen = "127.0.0.1:${port}"
max_drain_duration_ms = 2000
log_level = "error"
request_logger = false
[database]
url = "env:DATABASE_URL"
[limits]
default_query_limit = 50
max_query_limit = 50
[telemetry]
metrics_listen = "127.0.0.1:${metricsPort}"
sample_ratio = 0.0
${process.env.ANVIL_URL ? '[chains]\n"eip155:31337" = "env:ANVIL_URL"\n' : ""}
`,
  );

  const close = async () => {
    try {
      if (child && child.exitCode === null && child.signalCode === null) {
        child.kill("SIGKILL");
      }
      if (exited)
        await within(exited, 5_000, "Recovery backend cleanup timed out");
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  };
  const start = async () => {
    if (child && child.exitCode === null && child.signalCode === null) {
      throw new Error("Recovery backend is already running");
    }
    spawnError = undefined;
    diagnostic = "";
    const backendEnvironment = { ...process.env };
    delete backendEnvironment.OTEL_EXPORTER_OTLP_ENDPOINT;
    child = spawn(binary, ["--config-file", config], {
      stdio: ["ignore", "ignore", "pipe"],
      env: backendEnvironment,
    });
    child.stderr!.on("data", (bytes: Buffer) => {
      diagnostic = (diagnostic + bytes.toString()).slice(-8_192);
    });
    exited = new Promise<void>((resolve) => {
      child!.once("error", (error) => {
        spawnError = error;
        resolve();
      });
      child!.once("exit", () => resolve());
    });
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      if (spawnError) {
        throw new Error("Recovery backend failed to start", {
          cause: spawnError,
        });
      }
      if (child.exitCode !== null || child.signalCode !== null) {
        const safeDiagnostic = diagnostic.replaceAll(
          process.env.DATABASE_URL!,
          "<database URL>",
        );
        throw new Error(
          `Recovery backend exited before readiness: ${safeDiagnostic}`,
        );
      }
      try {
        const response = await fetch(
          `http://127.0.0.1:${metricsPort}/metrics`,
          {
            signal: AbortSignal.timeout(1_000),
          },
        );
        if ((await response.text()).includes("xmtp_backend_ready 1")) return;
      } catch {
        // Startup has not opened the metrics listener yet.
      }
      await sleep(100);
    }
    throw new Error("Recovery backend did not become ready");
  };
  try {
    await start();
  } catch (error) {
    await close();
    throw error;
  }
  return {
    url: `http://127.0.0.1:${port}`,
    start,
    async stopGracefully() {
      if (!child || !exited)
        throw new Error("Recovery backend was not started");
      if (!child.kill("SIGTERM"))
        throw new Error("Recovery backend signal failed");
      await within(exited, 15_000, "Recovery backend did not finish its drain");
      if (child.exitCode !== 0) {
        throw new Error(
          "Recovery backend did not exit successfully after its drain",
        );
      }
    },
    close,
  };
}
