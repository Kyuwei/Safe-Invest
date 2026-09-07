# Piloter Safe Invest avec une IA

Le même fichier qui ouvre la fenêtre est aussi un serveur
[MCP](https://modelcontextprotocol.io). Rien à installer en plus.

## Brancher un client

```json
{
  "mcpServers": {
    "safe-invest": {
      "command": "C:\\chemin\\vers\\safe-invest.exe",
      "args": ["mcp"]
    }
  }
}
```

Pour Claude Desktop, ce bloc va dans `claude_desktop_config.json`. Pour Claude Code,
dans le `.mcp.json` du projet. Redémarrez le client après modification.

Deux options utiles :

| Argument | Effet |
|---|---|
| `--demo` | Marché simulé, aucun appel réseau. Parfait pour essayer. |
| `--data-dir <chemin>` | Un dossier de parties séparé, pour ne pas toucher aux vraies. |

Par exemple, `"args": ["mcp", "--demo"]`.

## Vérifier que ça marche

```
safe-invest.exe doctor
```

Si le diagnostic passe, le serveur démarrera. Les journaux du serveur partent sur la
sortie d'erreur ; la sortie standard ne transporte que le protocole.

Sous Windows, l'invite de commandes n'attend pas une application fenêtrée : le texte du
diagnostic s'affiche bien, parfois juste après le retour du prompt. Utilisez
`start /wait safe-invest.exe doctor` si vous voulez que le terminal patiente. Cela ne
concerne que les sous-commandes console — un client MCP, lui, communique par des tuyaux
et attend correctement.

## Les quatorze outils

### Parties

| Outil | Ce qu'il fait |
|---|---|
| `list_games` | Les parties enregistrées, et laquelle est courante |
| `create_game` | Démarre une partie et en fait la partie courante |
| `open_game` | Rend courante une partie existante |
| `set_goal` | Fixe le montant à atteindre et la date limite |
| `end_game` | Termine la partie à sa valeur du moment ; plus aucun ordre ensuite |

### Lecture

| Outil | Ce qu'il fait |
|---|---|
| `get_portfolio` | Trésorerie, positions cotées au marché, plus-values latentes et réalisées |
| `get_goal_progress` | Avancement, jours restants, rendement encore nécessaire |
| `get_trade_history` | Historique daté, avec la justification de chaque opération |
| `get_market_sources` | Quelle source répond, laquelle est en échec et pourquoi |
| `get_summary` | Bilan d'une partie terminée : résultat, meilleur et pire trade, leçon |

### Marché

| Outil | Ce qu'il fait |
|---|---|
| `search_assets` | Cherche par symbole ou par nom |
| `list_popular_assets` | Le catalogue intégré : cryptos, actions, ETF connus |
| `get_quotes` | Cours actuels, avec leur source et le drapeau « simulé » |
| `get_price_history` | Clôtures quotidiennes, pour juger une tendance |

### Ordres

| Outil | Ce qu'il fait |
|---|---|
| `buy` | Achète une quantité (`quantity`) ou pour une somme (`amount`, frais compris) |
| `sell` | Vend une quantité, de quoi dégager une somme, ou tout (`all: true`) |

## La règle qui compte

En partie IA, `buy` et `sell` **refusent** un ordre sans `rationale` :

```json
{
  "symbol": "BTC",
  "kind": "crypto",
  "amount": 2500,
  "rationale": "Position crypto de cœur, plafonnée à 25 % pour limiter la volatilité."
}
```

Cette phrase apparaît telle quelle dans l'historique que lit la personne qui apprend.
Écrivez-la pour elle : ce qui a motivé la décision, pas ce que fait l'ordre. « Achat de
0,04 BTC » n'apprend rien à personne.

## Une partie type

```
list_games                      → aucune partie
create_game                     player_name "Claude", player_kind "ai",
                                  starting_cash 10000, target_amount 15000,
                                  deadline "2027-12-31"
get_market_sources              → vérifier que les cours sont réels
list_popular_assets  kind etf   → trouver un tracker monde
get_price_history    CW8.PA     → regarder la tendance
buy                  CW8.PA, amount 4000, rationale "…"
get_portfolio                   → vérifier le résultat
get_goal_progress               → +12 %/an encore nécessaires
…
get_summary                     → une fois la partie terminée, le bilan
```

## La fin d'une partie

Une partie se termine de trois façons, et dans les trois cas la valeur du portefeuille
est **figée à cet instant** : le bilan raconte ce qui s'est passé, il ne se recalcule pas
au cours du jour.

| Fin | Déclencheur |
| --- | --- |
| `goalReached` | Le montant visé est atteint, à la première évaluation qui le constate |
| `deadlinePassed` | La date limite est passée |
| `stopped` | `end_game`, ou le bouton « Terminer la partie » dans la fenêtre |

Les deux premières sont automatiques : il n'y a rien à appeler. `get_portfolio` renvoie
alors un champ `outcome`, et tout `buy` ou `sell` est refusé avec une phrase qui le dit.
Une IA qui vérifie `outcome` avant d'agir n'aura jamais à lire ce refus.

## Conventions d'arguments

**Les montants** acceptent un nombre ou une chaîne. `0.25` et `"0.25"` donnent
exactement la même valeur ; la chaîne évite qu'un flottant arrondisse une décimale.

**Les dates** acceptent `2027-12-31` — la fin de cette journée — ou un horodatage
complet `2027-12-31T18:00:00Z`.

**`game_id`** est facultatif partout : sans lui, l'outil agit sur la partie courante,
celle qu'a fixée `create_game` ou `open_game`.

**Les types d'actif** sont `crypto`, `stock` ou `etf`.

## Quand un outil refuse

Une erreur porte une phrase et souvent une suggestion :

```
Aucune partie n'est ouverte.
  hint: Appelez `list_games` puis `open_game`, ou `create_game` pour en démarrer une.
```

Les refus les plus courants :

| Message | Ce qu'il faut faire |
|---|---|
| « chaque opération doit être accompagnée d'une justification » | Ajouter `rationale` |
| « Trésorerie insuffisante » | Réduire le montant, ou vendre d'abord |
| « Aucune position sur X » | On ne peut pas vendre ce qu'on ne détient pas |
| « Aucun cours disponible » | Voir `get_market_sources` ; réessayer plus tard |
| « Précisez une seule façon de dimensionner » | `quantity` **ou** `amount`, pas les deux |

## Ce que l'IA voit, et ce qu'elle ne peut pas faire

Chaque cours porte `sourceId` et `isSimulated`. Un cours simulé vient du marché de
repli : il ne vaut que pour l'exercice, et il ne doit jamais être présenté comme réel.

Le serveur n'a accès qu'au dossier des parties et aux API de cours. Il ne peut ni lire
d'autres fichiers, ni exécuter de commandes, ni passer le moindre ordre réel.
