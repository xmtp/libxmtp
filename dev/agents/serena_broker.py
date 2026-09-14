"""Connect stdio clients to one read-only Serena process per worktree."""

import asyncio
import fcntl
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import time

import psutil

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / ".cache/agents/serena"
TOOLS = {
    "find_symbol",
    "get_symbols_overview",
    "find_referencing_symbols",
    "find_declaration",
    "get_diagnostics_for_file",
}
INSTRUCTIONS = (
    "Read-only Rust navigation for this libxmtp worktree. Diagnostics are advisory. "
    "Empty diagnostics do not replace focused Cargo checks and tests."
)


def identity():
    """Reject reuse after changes to the launcher, runtime, or Nix environment."""
    digest = hashlib.sha256(os.environ.get("XMTP_NIX_WRAPPER_ID", "").encode())
    for name in ("serena_broker.py", "context.yml", "uv.lock", "pyproject.toml"):
        digest.update((ROOT / "dev/agents" / name).read_bytes())
    digest.update(sys.executable.encode())
    return digest.hexdigest()


def live_process(record):
    """Check process creation time before reusing or stopping a PID."""
    if record.get("root") != str(ROOT):
        return None
    try:
        process = psutil.Process(record["pid"])
        if (
            process.create_time() == record["created"]
            and process.is_running()
            and process.status() != psutil.STATUS_ZOMBIE
        ):
            return process
    except (KeyError, psutil.NoSuchProcess):
        pass
    return None


def read_record():
    try:
        return json.loads((STATE / "server.json").read_text())
    except FileNotFoundError:
        return {}


def ensure_server():
    """Serialize startup so simultaneous clients cannot start extra analyzers."""
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (STATE / "lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        record = read_record()
        if live_process(record):
            if record["identity"] != identity():
                raise RuntimeError(
                    "Serena inputs changed. Run dev/serena stop, then reconnect."
                )
            return record
        (STATE / "server.json").unlink(missing_ok=True)
        with (STATE / "server.log").open("w") as log:
            process = subprocess.Popen(
                [sys.executable, str(Path(__file__).resolve()), "--serve"],
                cwd=ROOT,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=log,
                start_new_session=True,
            )
        deadline = time.monotonic() + 150
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(
                    f"Serena startup failed. Read {STATE / 'server.log'}"
                )
            record = read_record()
            if record.get("pid") == process.pid:
                return record
            time.sleep(0.1)
        process.terminate()
        raise RuntimeError(f"Serena startup timed out. Read {STATE / 'server.log'}")


def configure():
    """Resolve rust-analyzer inside Nix. Do not let Serena install a toolchain."""
    analyzer = shutil.which("rust-analyzer")
    if not analyzer or not str(Path(analyzer).resolve()).startswith("/nix/store/"):
        raise RuntimeError("Expected the Nix rust-analyzer. Use dev/serena.")
    os.environ["SERENA_HOME"] = str(STATE / "home")
    os.environ["SQLX_OFFLINE"] = "true"
    config = {
        "projects": [],
        "project_serena_folder_location": str(STATE / "project"),
        "web_dashboard": False,
        "web_dashboard_open_on_launch": False,
        "gui_log_window": False,
        "base_modes": [],
        "default_modes": [],
        "fixed_tools": sorted(TOOLS),
        "ls_specific_settings": {
            "rust": {
                "ls_path": analyzer,
                "initializationOptions": {
                    "checkOnSave": False,
                    "cargo": {
                        "buildScripts": {"enable": True},
                        "extraEnv": {"SQLX_OFFLINE": "true"},
                    },
                    "procMacro": {"enable": True},
                },
            }
        },
    }
    for path, data in (
        (STATE / "home/serena_config.yml", config),
        (
            STATE / "project/project.yml",
            {
                "project_name": "libxmtp",
                "language_servers": ["rust"],
                "read_only": True,
            },
        ),
    ):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data))


def serve():
    """Bind an OS-selected loopback port and require a per-worktree secret."""
    configure()
    from serena.mcp import SerenaMCPFactory
    import uvicorn

    token = secrets.token_urlsafe(32)
    factory = SerenaMCPFactory(
        transport="streamable-http",
        context=str(ROOT / "dev/agents/context.yml"),
        project=str(ROOT),
    )
    server = factory.create_mcp_server()
    app = server.streamable_http_app()

    async def authenticated(scope, receive, send):
        if scope["type"] == "http":
            headers = dict(scope["headers"])
            if headers.get(b"authorization") != f"Bearer {token}".encode():
                await send(
                    {"type": "http.response.start", "status": 403, "headers": []}
                )
                await send({"type": "http.response.body", "body": b"Forbidden"})
                return
        await app(scope, receive, send)

    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
        record = {
            "root": str(ROOT),
            "pid": os.getpid(),
            "created": psutil.Process().create_time(),
            "identity": identity(),
            "url": f"http://127.0.0.1:{port}/mcp",
            "token": token,
        }
        temporary = STATE / "server.json.tmp"
        temporary.write_text(json.dumps(record))
        temporary.chmod(0o600)
        temporary.replace(STATE / "server.json")
        try:
            uvicorn.Server(uvicorn.Config(authenticated, log_level="warning")).run(
                sockets=[listener]
            )
        finally:
            if factory.agent is not None:
                factory.agent.on_shutdown()
            (STATE / "server.json").unlink(missing_ok=True)


async def proxy(record):
    from mcp import ClientSession
    from mcp.client.streamable_http import streamable_http_client
    from mcp.server import Server
    from mcp.server.stdio import stdio_server
    import httpx

    # The child publishes its endpoint immediately before starting the listener.
    for _ in range(100):
        try:
            with socket.create_connection(
                ("127.0.0.1", int(record["url"].split(":")[2].split("/")[0])), timeout=1
            ):
                break
        except OSError:
            await asyncio.sleep(0.1)
    async with httpx.AsyncClient(
        headers={"Authorization": f"Bearer {record['token']}"}, timeout=240
    ) as http:
        async with streamable_http_client(record["url"], http_client=http) as (
            read,
            write,
            _,
        ):
            async with ClientSession(read, write) as session:
                await session.initialize()
                broker = Server("libxmtp-serena", instructions=INSTRUCTIONS)

                @broker.list_tools()
                async def list_tools():
                    return [
                        tool
                        for tool in (await session.list_tools()).tools
                        if tool.name in TOOLS
                    ]

                @broker.call_tool()
                async def call_tool(name, arguments):
                    if name not in TOOLS:
                        raise ValueError(f"Tool is not allowed: {name}")
                    return await session.call_tool(name, arguments)

                async with stdio_server() as (stdin, stdout):
                    await broker.run(
                        stdin, stdout, broker.create_initialization_options()
                    )


def stop():
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (STATE / "lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        process = live_process(read_record())
        if process:
            process.terminate()
            try:
                process.wait(timeout=30)
            except psutil.TimeoutExpired:
                raise RuntimeError(
                    "Serena has not stopped. Check server.log before retrying."
                ) from None
        print("Serena stopped for this worktree.", file=sys.stderr)


if __name__ == "__main__":
    os.umask(0o077)
    action = sys.argv[1] if len(sys.argv) > 1 else "connect"
    if action == "--serve":
        serve()
    elif action == "stop":
        stop()
    elif action == "connect":
        asyncio.run(proxy(ensure_server()))
    elif action == "smoke":
        from smoke_serena import main

        asyncio.run(main())
    else:
        sys.exit("Usage: dev/serena [connect|stop|smoke]")
