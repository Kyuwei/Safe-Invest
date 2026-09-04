# Piloter Safe Invest avec une IA (MCP)

Safe Invest expose un **serveur MCP** : une IA peut créer une partie, consulter les cours
réels, acheter, vendre et suivre son objectif. Chaque opération qu'elle passe doit être
**justifiée**, et cette justification apparaît dans l'historique de l'application.

L'application de bureau n'a pas besoin d'être ouverte. Les deux programmes lisent et
écrivent les mêmes fichiers de partie ; si l'app tourne, elle se met à jour en direct
dès que l'IA agit.

## Compiler le serveur

```bash
dotnet build src/SafeInvest.Mcp -c Release
```

L'exécutable se trouve alors dans `src/SafeInvest.Mcp/bin/Release/net10.0/`.

## Le déclarer auprès d'un client MCP

### Claude Code / Claude Desktop (`.mcp.json` ou la configuration du client)

```json
{
  "mcpServers": {
    "safe-invest": {
      "command": "dotnet",
      "args": [
        "run",
        "--project",
        "C:\\chemin\\vers\\Safe-Invest\\src\\SafeInvest.Mcp\\SafeInvest.Mcp.csproj",
        "-c",
        "Release",
        "--no-build"
      ]
    }
  }
}
```

Ou, une fois publié, en pointant directement l'exécutable :

```json
{
  "mcpServers": {
    "safe-invest": {
      "command": "C:\\chemin\\vers\\SafeInvest.Mcp.exe"
    }
  }
}
```

### Variables d'environnement reconnues

| Variable | Rôle |
|---|---|
| `SAFEINVEST_DATA_DIR` | Déplace le dossier des parties (par défaut `%LOCALAPPDATA%\SafeInvest`) |
| `SAFEINVEST_COINGECKO_KEY` | Clé Demo CoinGecko, facultative |
| `SAFEINVEST_COINMARKETCAP_KEY` | Clé CoinMarketCap, facultative |
| `SAFEINVEST_FINNHUB_KEY` | Clé Finnhub, facultative |

Sans aucune clé, le serveur fonctionne : CoinGecko et Yahoo Finance sont interrogés sans
inscription. Voir [cles-api.md](cles-api.md).

## Les outils exposés

### Partie

| Outil | Ce qu'il fait |
|---|---|
| `list_games` | Liste les parties enregistrées, la plus récente d'abord |
| `create_game` | Démarre une partie et l'ouvre (capital, devise, type de joueur, objectif) |
| `open_game` | Change la partie courante — l'application suit |
| `get_portfolio` | Trésorerie, positions valorisées, plus-values, objectif |
| `set_goal` | Fixe le montant à atteindre et la date limite |
| `get_goal_progress` | Avancement, jours restants, rendement annuel encore nécessaire |
| `get_trade_history` | Opérations passées, datées et commentées |
| `get_market_sources` | État de santé de chaque source de données |

### Marché

| Outil | Ce qu'il fait |
|---|---|
| `search_assets` | Cherche une crypto, une action ou un ETF par nom ou par symbole |
| `get_quotes` | Cours actuels, variation 24 h, provenance du chiffre |
| `get_price_history` | Clôtures passées (jour, semaine, mois, trimestre, année) |
| `list_popular_assets` | Le catalogue intégré, pour démarrer sans rien connaître |

### Opérations

| Outil | Ce qu'il fait |
|---|---|
| `buy` | Achat par quantité **ou** par montant (frais compris) |
| `sell` | Vente par quantité, par montant, ou position entière (`all: true`) |

En partie IA, `buy` et `sell` **refusent** une opération sans `rationale`. C'est
volontaire : l'intérêt pédagogique du mode IA tient à ce que chaque décision soit
expliquée.

## Un tour de jeu typique

```
1. create_game { startingCash: 10000, playerKind: "ai",
                 goalAmount: 15000, goalDeadline: "2027-12-31" }
2. get_portfolio                       → où en est-on
3. search_assets { query: "solana" }   → trouver le bon symbole
4. get_quotes { symbols: ["BTC","SOL"], kind: "crypto" }
5. get_price_history { symbol: "BTC", range: "quarter" }
6. buy { symbol: "BTC", amount: 3000,
         rationale: "Position cœur sur l'actif le plus liquide du secteur." }
7. get_goal_progress                   → reste-t-on dans les clous
```

## Ce que renvoient les outils

Toutes les réponses sont du JSON avec un champ `ok`. En cas d'échec :

```json
{ "ok": false, "error": "Trésorerie insuffisante : il faudrait 3 200,00 EUR…",
  "hint": "Ajustez la quantité ou le montant, puis réessayez." }
```

Deux champs méritent l'attention de l'IA :

- `isSimulated` / `containsSimulatedPrices` — le cours est **inventé** parce qu'aucune
  source réelle n'était joignable. À signaler plutôt qu'à traiter comme un vrai prix.
- `direction` — `"up"`, `"down"` ou `"flat"`, la même information que le vert/rouge affiché
  à l'écran.

## Vérifier que tout fonctionne

```bash
dotnet build src/SafeInvest.Mcp -c Release
python3 scripts/mcp-smoke-test.py
```

Le script joue une partie complète — création, cours réels, achat refusé faute de
justification, achat, vente, historique — et sort en erreur au premier problème.
