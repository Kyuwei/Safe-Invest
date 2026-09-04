# Sources de données et clés API

Safe Invest affiche de **vrais cours**. Il fonctionne sans aucune inscription ; ajouter
une clé gratuite améliore surtout les limites d'appel.

## Sans rien faire

| Source | Actifs | Limite |
|---|---|---|
| **CoinGecko** (API publique sans clé) | cryptos | ~5 à 15 appels/minute |
| **Yahoo Finance** (endpoint `v8/chart`) | actions, ETF | souple, non documenté |
| **Frankfurter** (taux BCE) | conversion USD → EUR | aucune limite pratique |

C'est la configuration par défaut : l'application est utilisable dès le premier
lancement.

## Avec une clé gratuite

| Source | Où l'obtenir | Ce que ça apporte |
|---|---|---|
| **CoinGecko Demo** | coingecko.com → Developer Dashboard | 100 appels/minute au lieu de ~5 |
| **CoinMarketCap Basic** | coinmarketcap.com/api | 15 000 crédits/mois, source crypto alternative |
| **Finnhub** | finnhub.io | 60 appels/minute sur les actions américaines |

Les clés se saisissent dans **Réglages → Sources de données**. Elles sont chiffrées avec
DPAPI (liées à votre compte Windows) dans `%LOCALAPPDATA%\SafeInvest\settings.json`.

Pour le serveur MCP lancé depuis un terminal, les variables d'environnement
`SAFEINVEST_COINGECKO_KEY`, `SAFEINVEST_COINMARKETCAP_KEY` et `SAFEINVEST_FINNHUB_KEY`
prennent le relais si aucune clé n'est enregistrée.

## L'ordre des sources

Chaque famille d'actifs a sa cascade, modifiable dans les Réglages :

```
Cryptos : coingecko → coinmarketcap → repli web → simulé
Actions : yahoo     → finnhub       → repli web → simulé
```

À chaque échec — panne, quota épuisé, page qui a changé — on passe au suivant.

### Le repli web

Quand toutes les API sont indisponibles, Safe Invest lit le cours directement sur une page
publique (CoinMarketCap pour les cryptos, stockanalysis.com pour les actions). C'est un
**filet de sécurité**, pas une source de référence : la mise en page de ces sites peut
changer du jour au lendemain. Les sélecteurs sont regroupés dans
`src/SafeInvest.MarketData/Providers/WebScrapeProvider.cs` pour qu'une réparation tienne
en une ligne.

### Le mode simulé

Dernier recours, et aussi un mode qu'on peut activer volontairement (Réglages → Mode
simulé) pour une démonstration sans réseau. Les cours sont générés localement, de façon
déterministe : le même actif à la même minute vaut la même chose sur toutes les machines,
ce qui permet à une classe entière de voir les mêmes chiffres.

**Ces cours sont clairement signalés** — badge dans l'interface, `isSimulated: true` côté
MCP, mention sur l'opération dans l'historique. Un outil pédagogique ne doit jamais
laisser croire qu'un chiffre inventé est un vrai prix de marché.

## Diagnostiquer

- Dans l'application : **Réglages → Sources de données** affiche l'état de chaque source.
- Via MCP : l'outil `get_market_sources` renvoie la même information.
