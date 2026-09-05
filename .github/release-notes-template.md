Simulateur d'investissement pédagogique : de l'argent **fictif**, de **vrais** cours de
marché. Rien ne peut être perdu, et pourtant tout est vrai sauf l'argent.

## Installation

1. Télécharger `SafeInvest-{VERSION}-win-x64.zip`
2. Décompresser le dossier où vous voulez
3. Lancer `SafeInvest.exe`

Aucune installation, aucun certificat, aucune inscription à un service de données :
l'application interroge CoinGecko et Yahoo Finance sans clé API.

Windows 10 version 1809 (build 17763) ou plus récent, ou Windows 11. Le runtime .NET et
le Windows App SDK sont inclus dans l'archive.

## Faire jouer une IA

`SafeInvest-MCP-{VERSION}-win-x64.zip` contient le serveur MCP : une IA peut créer une
partie, consulter les cours, acheter et vendre, et l'application affiche chaque opération
en direct avec la justification donnée. Configuration du client dans
[docs/mcp.md](https://github.com/Kyuwei/Safe-Invest/blob/main/docs/mcp.md).

## Bon à savoir

L'archive de l'application pèse une centaine de mégaoctets, et ce n'est pas un `.exe`
isolé : une application WinUI 3 non empaquetée embarque le Windows App SDK à côté de son
exécutable. Gardez le dossier entier.

Les cours qui ne viennent pas d'une source réelle — mode démonstration, ou repli quand
toutes les sources sont indisponibles — sont signalés comme tels partout dans
l'interface.

---

L'argent est fictif, aucune transaction réelle n'est jamais passée, et rien dans cette
application ne constitue un conseil en investissement.
