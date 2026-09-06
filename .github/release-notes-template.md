Simulateur d'investissement pédagogique : de l'argent fictif, de vrais cours.

## Installation

Téléchargez **`safe-invest.exe`** et double-cliquez. C'est tout — un seul
fichier de {SIZE} Mo, rien à installer, rien à désinstaller. Vos parties sont
enregistrées dans `%LOCALAPPDATA%\SafeInvest`.

Windows 10 (2004 ou plus récent) et Windows 11 conviennent. L'application
s'appuie sur *Microsoft Edge WebView2*, présent d'origine sur Windows 11 et
installé avec Edge sur Windows 10. S'il manque, l'application vous le dit et
vous donne le lien.

En cas de doute, ouvrez un terminal dans le dossier du fichier et lancez :

```
safe-invest.exe doctor
```

Il affiche où sont vos données, si le moteur web est présent et quelles
sources de cours sont configurées.

## Faire jouer une IA

Le même fichier est aussi un serveur MCP. Dans la configuration de votre client
(Claude Desktop, `.mcp.json`, …) :

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

L'IA dispose alors des outils qu'il faut pour jouer : créer une partie,
chercher un actif, relever un cours, acheter, vendre, suivre l'objectif, clore
la partie et en lire le bilan. En partie IA, chaque opération doit être
accompagnée d'une justification, qui s'affiche dans l'historique — c'est ce qui
rend la partie lisible pour la personne qui apprend.

## Vérifier le téléchargement

```
certutil -hashfile safe-invest.exe SHA256
```

Doit donner :

```
{SHA256}
```

## Avertissement

L'argent est fictif. Les cours sont réels, mais ce programme n'est ni un
conseil en investissement ni un outil de gestion. Quand aucune source ne
répond, un marché simulé prend le relais et l'interface l'indique clairement.
