"""Check two independent stdio clients against the shared worktree server."""

import asyncio
import json
from unittest.mock import patch

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

from serena_broker import ROOT, STATE, TOOLS, ensure_server, live_process


async def query():
    parameters = StdioServerParameters(command=str(ROOT / "dev/serena"))
    async with stdio_client(parameters) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            names = {tool.name for tool in (await session.list_tools()).tools}
            assert names == TOOLS, names
            result = await session.call_tool(
                "find_symbol",
                {
                    "name_path_pattern": "Bucket",
                    "relative_path": "crates/xmtp_common/src/rate_limit.rs",
                },
            )
            assert not result.isError, result
            assert "Bucket" in str(result.content), result
            denied = await session.call_tool("activate_project", {"project": "/tmp"})
            assert denied.isError, denied
            return json.loads((STATE / "server.json").read_text())["pid"]


async def main():
    first, second = await asyncio.gather(query(), query())
    assert first == second, (first, second)
    record = json.loads((STATE / "server.json").read_text())
    assert live_process({**record, "created": record["created"] - 1}) is None
    assert live_process({**record, "root": "/other/worktree"}) is None
    with patch("serena_broker.identity", return_value="changed inputs"):
        try:
            ensure_server()
        except RuntimeError as error:
            assert "inputs changed" in str(error), error
        else:
            raise AssertionError("Changed inputs reused the server")
    print(
        f"PASS: two stdio clients share Serena PID {first}; five query tools; project switching blocked"
    )


if __name__ == "__main__":
    asyncio.run(main())
