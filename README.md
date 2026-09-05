# Safe Invest

Un simulateur d'investissement **pédagogique** : on y place une somme d'argent **fictive**
sur de **vrais** actifs — cryptomonnaies, actions, ETF — aux **cours réels du marché**.

Rien ne peut être perdu, et pourtant tout est vrai sauf l'argent. C'est fait pour
apprendre : comprendre ce qu'achète réellement un ordre, voir une ligne passer au vert ou
au rouge, et découvrir ce qu'un objectif de rendement exige vraiment.

Deux façons de jouer :

- **une personne** cherche un actif, achète, vend, et suit son portefeuille ;
- **une IA** joue à travers un **serveur MCP**, et l'application devient un écran
  d'observation : chaque opération s'affiche avec son cours et **la raison qui l'a
  motivée**.

Une partie IA peut recevoir une consigne chiffrée : *atteindre 15 000 € avant le
31 décembre 2027*. L'application montre en permanence l'avancement et le rendement annuel
que cet objectif réclame encore.

## Ce qu'il faut pour l'utiliser

- Windows 10 version 1809 (build 17763) ou plus récent, ou Windows 11
- Rien d'autre : l'application est livrée en dossier autonome, sans installation ni
  certificat, et fonctionne **sans aucune inscription** à un service de données

## Démarrage rapide

```bash
git clone https://github.com/Kyuwei/Safe-Invest.git
cd Safe-Invest

# L'application
dotnet publish src/SafeInvest.App -c Release -r win-x64
# puis lancer SafeInvest.exe dans le dossier publié

# Le serveur MCP, pour faire jouer une IA
dotnet build src/SafeInvest.Mcp -c Release
```

La CI publie aussi un dossier `win-x64` prêt à l'emploi en artefact de build.

## Comment c'est construit

| Projet | Rôle |
|---|---|
| `src/SafeInvest.Core` | Le domaine et les règles du jeu : actifs, ordres, portefeuille, objectif, sauvegardes |
| `src/SafeInvest.MarketData` | Les cours réels : plusieurs sources en cascade, cache, conversion de devises |
| `src/SafeInvest.App` | L'application WinUI 3 |
| `src/SafeInvest.Mcp` | Le serveur MCP qui permet à une IA de jouer |

Le point important : **l'application et l'IA passent par le même moteur**. Un ordre passé
à la souris et un ordre passé par une IA suivent exactement les mêmes règles, les mêmes
frais et les mêmes contrôles.

Les deux programmes lisent et écrivent le même dossier de parties
(`%LOCALAPPDATA%\SafeInvest`). L'application surveille ce dossier : quand l'IA agit, le
tableau de bord se met à jour dans la seconde, sans rien faire.

### Les cours

Par défaut, sans aucune clé API :

- **CoinGecko** pour les cryptomonnaies
- **Yahoo Finance** pour les actions et les ETF
- **Frankfurter** (taux de la BCE) pour convertir les cours en dollars vers l'euro

Si une source tombe ou épuise son quota, on passe à la suivante : CoinMarketCap ou Finnhub
si une clé gratuite a été saisie, puis un **repli par lecture de pages web publiques**, et
en tout dernier recours des **cours simulés** pour que l'application reste utilisable hors
ligne.

Les cours simulés sont **signalés partout** où ils apparaissent — bandeau, badge sur la
position, mention sur l'opération. Un outil pédagogique ne doit jamais laisser croire
qu'un chiffre inventé est un vrai prix de marché.

Détails et clés facultatives : [`docs/cles-api.md`](docs/cles-api.md).

## Faire jouer une IA

Le serveur MCP expose quatorze outils : créer une partie, chercher un actif, lire les
cours et l'historique, acheter, vendre, suivre l'objectif.

En partie IA, `buy` et `sell` **refusent** un ordre sans justification. C'est délibéré :
tout l'intérêt du mode IA tient à ce que l'historique se lise comme une suite de décisions
expliquées.

Configuration du client MCP et liste complète des outils :
[`docs/mcp.md`](docs/mcp.md).

## Documentation

- [Guide de l'utilisateur](docs/guide-utilisateur.md) — jouer, comprendre les écrans
- [Piloter avec une IA (MCP)](docs/mcp.md)
- [Sources de données et clés API](docs/cles-api.md)

## Développement

```bash
# Tests (multiplateforme, tournent aussi sous Linux et macOS)
dotnet test tests/SafeInvest.Core.Tests
dotnet test tests/SafeInvest.MarketData.Tests

# Test de bout en bout du serveur MCP, contre les vraies API
python3 scripts/mcp-smoke-test.py

# La même chose sans réseau
python3 scripts/mcp-smoke-test.py --simulated

# Vérifier le C# de l'application WinUI depuis Linux ou macOS
./scripts/typecheck-app.sh
```

L'application WinUI ne se compile entièrement que sous Windows, parce que le compilateur
XAML n'existe que là. La CI s'en charge : les bibliothèques et les tests tournent sous
Linux, l'application est compilée et publiée sous `windows-latest`.

`scripts/typecheck-app.sh` comble l'écart pour le développement quotidien : il recrée les
membres que le compilateur XAML aurait générés et vérifie tout le C# de l'application en
une trentaine de secondes, sans Windows. Le XAML lui-même — balisage, chemins de binding,
ressources manquantes — reste vérifié par la CI Windows.

### Si la compilation Windows échoue avec `WMC9999`

`dotnet build` exécute le compilateur XAML dans un processus séparé qui n'arrive pas à
charger les ressources de ses propres messages : toute erreur XAML réelle ressort alors
comme « Xaml Internal Error WMC9999 », sans indiquer ni le fichier ni la cause.

La CI contourne le problème : en cas d'échec, elle recompile l'application avec MSBuild de
Visual Studio, qui exécute le même compilateur en processus et affiche l'erreur réelle. En
local, la même commande fonctionne :

```powershell
MSBuild.exe src\SafeInvest.App\SafeInvest.App.csproj /p:Configuration=Release /p:UseXamlCompilerExecutable=false
```

## Avertissement

Safe Invest est un **jeu éducatif**. L'argent est fictif, aucune transaction réelle n'est
jamais passée, et rien dans cette application ne constitue un conseil en investissement.
