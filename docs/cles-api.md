# Sources de cours et clés

**Safe Invest fonctionne entièrement sans aucune clé.** Cette page n'est utile que si
vous voulez plus de requêtes par minute ou une source supplémentaire.

## Ce qui se passe sans rien configurer

| Type d'actif | Source | Quota |
|---|---|---|
| Cryptomonnaies | CoinGecko, palier public | environ 5 à 15 requêtes par minute |
| Actions et ETF | Yahoo Finance | non documenté, généreux |
| Conversion de devises | Frankfurter (taux BCE) | aucun |

Si une source ne répond pas, la suivante prend le relais : CoinMarketCap ou Finnhub si
une clé est saisie, puis la lecture de pages web publiques, puis le marché simulé.

## La cascade, en entier

**Cryptomonnaies** — CoinGecko → CoinMarketCap → lecture de page → simulé
**Actions et ETF** — Yahoo Finance → Finnhub → lecture de page → simulé

L'ordre se modifie dans le fichier de réglages ; le simulateur est toujours ajouté en
dernier, même si on l'enlève, pour que l'application ne reste jamais muette.

## Ajouter une clé

Dans **Réglages → Clés d'API**. Une clé est chiffrée avec DPAPI sous votre compte
Windows et **n'est jamais réaffichée** : la case reste vide, avec la mention
« enregistrée ». Pour la retirer, enregistrez une valeur vide.

### Les clés possibles

| Source | Où l'obtenir | Ce que ça apporte |
|---|---|---|
| CoinGecko Demo | [coingecko.com/api/pricing](https://www.coingecko.com/en/api/pricing) | environ 30 requêtes par minute au lieu de 5 |
| CoinMarketCap | [coinmarketcap.com/api](https://coinmarketcap.com/api/) | ~15 000 crédits par mois, source crypto de secours |
| Finnhub | [finnhub.io/register](https://finnhub.io/register) | 60 requêtes par minute sur les valeurs américaines |

Toutes sont gratuites et demandent une inscription par courriel.

## Par variable d'environnement

Pratique pour une machine de test ou un serveur d'intégration, où l'on ne veut rien
écrire sur le disque :

```
SAFEINVEST_COINGECKO_KEY
SAFEINVEST_COINMARKETCAP_KEY
SAFEINVEST_FINNHUB_KEY
```

Une clé enregistrée dans les réglages a la priorité. La variable n'est consultée que si
rien n'est stocké — une variable d'environnement ne peut donc pas éclipser en silence la
clé que vous avez saisie.

## Le marché simulé

Quand aucune source ne répond, l'application invente des cours : une marche déterministe
ancrée sur des ordres de grandeur réalistes, pour qu'un bitcoin simulé coûte 68 000 € et
non 16 €.

**Ces cours sont signalés partout** : bandeau sur le tableau de bord, mention sur chaque
position, note sur l'opération dans l'historique, drapeau `isSimulated` dans les réponses
MCP.

Pour y jouer volontairement — apprendre hors ligne, faire une démonstration — cochez
**Mode démonstration** dans les réglages, ou lancez `safe-invest.exe --demo`.

## Ce que valent ces sources

Les points d'accès de Yahoo Finance et la lecture de pages publiques ne sont **pas des
API officielles**. Ils peuvent changer sans préavis. C'est précisément pourquoi il y a
une cascade et un simulateur derrière — et pourquoi **Réglages → Sources de cours**
affiche un voyant par source, avec la raison du dernier échec.

Aucune de ces sources n'est un flux de marché professionnel. Les cours sont réels, mais
retardés et arrondis. Cela suffit largement pour apprendre ; cela ne suffirait pour rien
d'autre.
