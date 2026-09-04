#!/usr/bin/env python3
"""Drives the Safe Invest MCP server over stdio and plays a full round.

Runs the same sequence an AI player would: open a game, look at the market, buy,
check the portfolio, sell, and read the history back. Used as the end-to-end check
in CI and as a way to see the server working locally.

    python3 scripts/mcp-smoke-test.py [--dotnet path/to/dotnet]

Exits non-zero on the first failed step.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from typing import Any

PROTOCOL_VERSION = "2025-06-18"


class McpClient:
    """Minimal newline-delimited JSON-RPC client, enough to exercise the server."""

    def __init__(self, process: subprocess.Popen[str]) -> None:
        self._process = process
        self._next_id = 0

    def _send(self, payload: dict[str, Any]) -> None:
        assert self._process.stdin is not None
        self._process.stdin.write(json.dumps(payload) + "\n")
        self._process.stdin.flush()

    def _read(self) -> dict[str, Any]:
        assert self._process.stdout is not None
        while True:
            line = self._process.stdout.readline()
            if not line:
                raise RuntimeError("le serveur MCP a fermé sa sortie standard")
            line = line.strip()
            if not line:
                continue
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                # Anything that is not JSON on stdout would be a protocol bug.
                raise RuntimeError(f"sortie non-JSON sur stdout : {line[:200]}") from None

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self._next_id += 1
        request_id = self._next_id
        self._send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params or {}})

        while True:
            message = self._read()
            if message.get("id") == request_id:
                if "error" in message:
                    raise RuntimeError(f"{method} a échoué : {message['error']}")
                return message.get("result", {})

    def notify(self, method: str, params: dict[str, Any] | None = None) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    def call_tool(self, name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        blocks = result.get("content", [])
        text = next((b.get("text", "") for b in blocks if b.get("type") == "text"), "")
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            raise RuntimeError(f"{name} n'a pas renvoyé de JSON : {text[:300]}") from None


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)
    print(f"  ok  {message}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dotnet", default=shutil.which("dotnet") or "dotnet")
    parser.add_argument("--configuration", default="Release")
    args = parser.parse_args()

    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    project = os.path.join(repo_root, "src", "SafeInvest.Mcp", "SafeInvest.Mcp.csproj")

    data_dir = tempfile.mkdtemp(prefix="safeinvest-smoke-")
    environment = {
        **os.environ,
        # Keep the smoke test out of the real save folder.
        "SAFEINVEST_DATA_DIR": data_dir,
        "DOTNET_NOLOGO": "1",
        "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
    }

    print(f"Données de test : {data_dir}")
    print("Démarrage du serveur MCP…")

    process = subprocess.Popen(
        [args.dotnet, "run", "--project", project, "-c", args.configuration, "--no-build"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        env=environment,
        cwd=repo_root,
    )

    try:
        client = McpClient(process)

        initialised = client.request(
            "initialize",
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "safe-invest-smoke-test", "version": "1.0"},
            },
        )
        client.notify("notifications/initialized")
        print(f"Serveur : {initialised.get('serverInfo', {})}")

        print("\n1. Outils exposés")
        tools = {t["name"] for t in client.request("tools/list").get("tools", [])}
        expected = {
            "list_games", "create_game", "open_game", "get_portfolio", "set_goal",
            "get_goal_progress", "get_trade_history", "get_market_sources",
            "search_assets", "get_quotes", "get_price_history", "list_popular_assets",
            "buy", "sell",
        }
        missing = expected - tools
        check(not missing, f"les {len(expected)} outils attendus sont exposés")

        print("\n2. Création d'une partie IA avec objectif")
        game = client.call_tool("create_game", {
            "startingCash": 10000,
            "playerName": "Claude",
            "playerKind": "ai",
            "currency": "EUR",
            "goalAmount": 15000,
            "goalDeadline": "2027-12-31",
        })
        check(game.get("ok") is True, "la partie est créée")
        check(game["cash"] == 10000, "la trésorerie de départ est de 10 000 €")
        check(game["playerKind"] == "Ai", "le joueur est bien une IA")
        check(game["goal"]["targetAmount"] == 15000, "l'objectif de 15 000 € est enregistré")
        game_id = game["gameId"]

        print("\n3. Cours de marché")
        quotes = client.call_tool("get_quotes", {"symbols": ["BTC", "ETH"], "kind": "crypto"})
        check(quotes.get("ok") is True, "les cours sont récupérés")
        check(len(quotes["quotes"]) >= 1, "au moins une cotation est revenue")
        btc = next(q for q in quotes["quotes"] if q["symbol"] == "BTC")
        check(btc["price"] > 0, f"BTC cote {btc['price']:,.2f} {btc['currency']} (source : {btc['source']})")
        check(btc["direction"] in {"up", "down", "flat"}, f"la tendance 24 h est « {btc['direction']} »")

        print("\n4. Un achat sans justification doit être refusé en partie IA")
        refused = client.call_tool("buy", {"symbol": "BTC", "amount": 1000})
        check(refused.get("ok") is False, "l'achat est refusé")
        check("justification" in refused["error"], f"le message l'explique : {refused['error']}")

        print("\n5. Achat justifié")
        bought = client.call_tool("buy", {
            "symbol": "BTC",
            "amount": 3000,
            "rationale": "Ouverture d'une position cœur sur la crypto la plus liquide.",
        })
        check(bought.get("ok") is True, "l'achat passe")
        check(bought["trade"]["total"] <= 3000, f"le coût total ({bought['trade']['total']:,.2f} €) ne dépasse pas le montant demandé")
        check(bought["trade"]["rationale"].startswith("Ouverture"), "la justification est conservée")
        check(bought["cashAfter"] < 10000, f"la trésorerie tombe à {bought['cashAfter']:,.2f} €")

        print("\n6. Achat d'une action, converti en euros")
        stock = client.call_tool("buy", {
            "symbol": "MSFT",
            "kind": "stock",
            "amount": 2000,
            "rationale": "Diversification hors crypto sur une valeur technologique.",
        })
        check(stock.get("ok") is True, "l'achat de MSFT passe")
        check(stock["currency"] == "EUR", "l'opération est bien libellée en euros")

        print("\n7. État du portefeuille")
        portfolio = client.call_tool("get_portfolio")
        check(portfolio.get("ok") is True, "le portefeuille est lisible")
        check(len(portfolio["positions"]) == 2, "les deux positions sont présentes")
        check(abs(portfolio["totalValue"] - (portfolio["cash"] + portfolio["investedValue"])) < 0.01,
              f"valeur totale cohérente : {portfolio['totalValue']:,.2f} €")
        check(portfolio["goal"]["daysRemaining"] > 0, f"il reste {portfolio['goal']['daysRemaining']} jours pour l'objectif")
        check(portfolio["goal"]["status"] in {"OnTrack", "Behind", "Achieved", "Expired"},
              f"statut de l'objectif : {portfolio['goal']['status']}")

        print("\n8. Vente partielle")
        sold = client.call_tool("sell", {
            "symbol": "BTC",
            "all": True,
            "rationale": "Prise de bénéfice pour sécuriser la progression vers l'objectif.",
        })
        check(sold.get("ok") is True, "la vente passe")
        check(sold["trade"]["side"] == "Sell", "l'opération est bien une vente")
        check(sold["trade"]["realizedPnL"] is not None, f"le résultat réalisé est calculé : {sold['trade']['realizedPnL']:,.2f} €")

        print("\n9. Historique daté et commenté")
        history = client.call_tool("get_trade_history")
        check(history["totalTrades"] == 3, "les trois opérations sont enregistrées")
        check(all(t["rationale"] for t in history["trades"]), "chaque opération porte sa justification")
        check(all(t["by"] == "Ai" for t in history["trades"]), "l'auteur est bien l'IA")
        for trade in history["trades"]:
            print(f"       {trade['timestamp'][:19]}  {trade['side']:4}  {trade['symbol']:6}  "
                  f"{trade['total']:>10,.2f} €  « {trade['rationale'][:52]}… »")

        print("\n10. La partie est bien celle que l'application afficherait")
        games = client.call_tool("list_games")
        current = next(g for g in games["games"] if g["gameId"] == game_id)
        check(current["isCurrent"] is True, "la partie créée est la partie courante")
        check(games["currentGameId"] == game_id, "le pointeur partagé avec l'app est à jour")

        print("\n11. État des sources de données")
        sources = client.call_tool("get_market_sources")
        for source in sources["sources"]:
            succeeded = source.get("lastCallSucceeded")
            state = "jamais appelée" if succeeded is None else (
                "OK" if succeeded else f"échec : {source.get('lastError', '')[:60]}")
            print(f"       {source['displayName']:34} {state}")
        check(len(sources["sources"]) >= 5, "toutes les sources sont déclarées")

        print("\nTest de fumée MCP : succès complet.")
        return 0

    except (AssertionError, RuntimeError) as error:
        print(f"\nÉCHEC : {error}", file=sys.stderr)
        return 1
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
        shutil.rmtree(data_dir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
