#!/usr/bin/env python3
"""Times a batch of MCP tool calls and reads the server's memory from /proc.

Called by scripts/profile.sh; usable on its own:

    python3 scripts/profile_mcp.py target/release/safe-invest /tmp/data
"""

from __future__ import annotations

import json
import subprocess
import sys
import time

CALLS = 50


def main() -> None:
    binary, data = sys.argv[1], sys.argv[2]
    server = subprocess.Popen(
        [binary, "mcp", "--demo", "--data-dir", data],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )

    state = {"id": 0}

    def rpc(method: str, params: dict) -> dict:
        state["id"] += 1
        request = {"jsonrpc": "2.0", "id": state["id"], "method": method, "params": params}
        server.stdin.write(json.dumps(request) + "\n")
        server.stdin.flush()
        while True:
            message = json.loads(server.stdout.readline())
            if message.get("id") == state["id"]:
                return message

    def kib(field: str) -> int:
        """One field of the server's /proc status, in kibibytes."""
        try:
            with open(f"/proc/{server.pid}/status", encoding="utf-8") as status:
                for line in status:
                    if line.startswith(field):
                        return int(line.split()[1])
        except OSError:
            pass
        return 0

    rpc(
        "initialize",
        {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "profile", "version": "1"},
        },
    )
    server.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
    server.stdin.flush()
    idle = kib("VmRSS")

    rpc(
        "tools/call",
        {
            "name": "create_game",
            "arguments": {"player_name": "Profil", "player_kind": "ai", "starting_cash": 10000},
        },
    )

    started = time.perf_counter()
    for _ in range(CALLS):
        rpc(
            "tools/call",
            {"name": "get_quotes", "arguments": {"symbols": ["BTC", "ETH", "SOL"], "kind": "crypto"}},
        )
    per_call = (time.perf_counter() - started) / CALLS * 1000

    print(f"  {'get_quotes (3 symboles, en cache)':<34} {per_call:.2f} ms par appel")
    print(f"  {'mémoire au repos':<34} {idle / 1024:.1f} Mio")
    print(f"  {'mémoire maximale':<34} {kib('VmHWM') / 1024:.1f} Mio")

    server.terminate()
    server.wait(timeout=5)


if __name__ == "__main__":
    main()
