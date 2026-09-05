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

## Installer

Téléchargez **`safe-invest.exe`** depuis la
[dernière version](https://github.com/Kyuwei/Safe-Invest/releases/latest) et
double-cliquez. Un seul fichier, une dizaine de mégaoctets, rien à installer.

Windows 10 (version 2004 ou plus récente) ou Windows 11. L'application s'appuie sur
*Microsoft Edge WebView2*, présent d'origine sur Windows 11 et installé avec Edge sur
Windows 10. S'il manque, l'application le dit et donne le lien.

Vos parties sont dans `%LOCALAPPDATA%\SafeInvest`. Pour désinstaller : supprimez le
fichier, et ce dossier si vous ne voulez rien garder.

En cas de doute :

```
safe-invest.exe doctor
```

affiche où sont vos données, si le moteur web est présent et quelles sources de cours
sont configurées.

> Une particularité de Windows : Safe Invest est une application fenêtrée, donc le double-clic
> n'ouvre pas de console noire — mais en contrepartie l'invite de commandes **ne l'attend pas**.
> Le texte s'affiche bien, parfois juste après que le prompt soit revenu. Pour que le terminal
> attende vraiment : `start /wait safe-invest.exe doctor`.

## Comment c'est construit

Rust, un seul exécutable, et pas de dépendance npm dans ce qui est livré.

| Bibliothèque | Rôle |
|---|---|
| `crates/platform` | Le code système : DPAPI, console. **Tout le `unsafe` du projet est là**, et nulle part ailleurs |
| `crates/core` | Le domaine et les règles : actifs, ordres, coût moyen, frais, objectif, sauvegardes |
| `crates/market` | Les cours réels : six sources en cascade, cache, limiteur de débit, conversion de devises |
| `crates/service` | Les opérations, écrites une fois : créer une partie, coter, acheter, vendre |
| `crates/mcp` | Les quatorze outils MCP, une coquille sur `service` |
| `crates/app` | L'exécutable : la fenêtre Tauri, et le serveur MCP en sous-commande |

Le point important : **la fenêtre et l'IA appellent les mêmes fonctions**. Un ordre passé
à la souris et un ordre passé par une IA suivent les mêmes règles, les mêmes frais et les
mêmes contrôles, parce qu'il n'existe qu'un seul chemin vers le moteur.

Un seul fichier fait les deux :

```
safe-invest.exe          ouvre la fenêtre
safe-invest.exe mcp      parle le protocole MCP sur l'entrée et la sortie standard
safe-invest.exe doctor   affiche un diagnostic
```

Les deux modes lisent et écrivent le même dossier de parties. L'application le surveille :
quand l'IA agit dans son processus, le tableau de bord se met à jour dans la seconde.

### Les cours

Par défaut, sans aucune clé :

- **CoinGecko** pour les cryptomonnaies
- **Yahoo Finance** pour les actions et les ETF
- **Frankfurter** (taux de la BCE) pour convertir vers l'euro

Si une source tombe ou épuise son quota, on passe à la suivante : CoinMarketCap ou Finnhub
si une clé gratuite a été saisie, puis un **repli par lecture de pages web publiques**, et
en tout dernier recours un **marché simulé** pour que l'application reste utilisable hors
ligne.

Les cours simulés sont **signalés partout** où ils apparaissent — bandeau, badge sur la
position, mention sur l'opération. Un outil pédagogique ne doit jamais laisser croire
qu'un chiffre inventé est un vrai prix de marché.

Détails et clés facultatives : [`docs/cles-api.md`](docs/cles-api.md).

## Faire jouer une IA

Dans la configuration de votre client MCP :

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

Le serveur expose quatorze outils : créer une partie, chercher un actif, lire les cours et
l'historique, acheter, vendre, suivre l'objectif.

En partie IA, `buy` et `sell` **refusent** un ordre sans justification. C'est délibéré :
tout l'intérêt du mode IA tient à ce que l'historique se lise comme une suite de décisions
expliquées.

Liste complète des outils et exemples : [`docs/mcp.md`](docs/mcp.md).

## Documentation

- [Guide de l'utilisateur](docs/guide-utilisateur.md) — jouer, comprendre les écrans
- [Piloter avec une IA (MCP)](docs/mcp.md)
- [Sources de données et clés API](docs/cles-api.md)
- [Sécurité](docs/securite.md) — ce qui est protégé, et comment
- [Performance](docs/performance.md) — les mesures, et la méthode pour les refaire

## Développement

Il faut Rust — la chaîne exacte est épinglée dans `rust-toolchain.toml`, `rustup`
l'installe tout seul.

```bash
cargo test --workspace          # 141 tests, sans réseau
node --test crates/app/ui/tests/*.test.js   # les tests de l'interface
cargo clippy --workspace --all-targets
cargo fmt --all
cargo build --release           # produit un seul exécutable
```

Le code spécifique à Windows se vérifie depuis n'importe quelle machine, parce qu'il est
isolé dans un crate qui ne dépend que de `windows-sys` :

```bash
rustup target add x86_64-pc-windows-msvc
cargo clippy -p safe-invest-platform -p safe-invest-core --target x86_64-pc-windows-msvc
```

Sous Linux, la fenêtre passe par WebKitGTK :

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev
```

Sans ces bibliothèques, tout le reste se compile et se teste quand même :

```bash
cargo test --workspace --no-default-features   # binaire console, sans fenêtre
```

Autres outils :

```bash
./scripts/profile.sh              # taille, démarrage, mémoire (voir docs/performance.md)
cargo deny check                  # licences, sources, avis de sécurité
cargo audit                       # vulnérabilités connues
python3 scripts/generate-icons.py # régénère l'icône
```

Les tests couvrent les règles qu'un joueur pourrait voir se casser : l'argent conservé sur
un aller-retour, l'achat « pour 100 € » qui ne dépasse jamais 100 €, l'IA à qui l'on
refuse un ordre qu'elle ne justifie pas, un actif non coté signalé plutôt que valorisé à
zéro, et deux cents écritures concurrentes qui arrivent toutes. Un test lance le vrai
binaire et joue une partie entière par-dessus les tuyaux MCP.

### Publier une version

```bash
git tag v0.2.0 && git push origin v0.2.0
```

ou, depuis l'onglet **Actions** de GitHub, lancer **Release** à la main en saisissant la
version. Le workflow vérifie que l'étiquette correspond à la version du `Cargo.toml`,
compile, contrôle que l'exécutable démarre, puis publie `safe-invest.exe` et son
empreinte SHA-256. Les notes se modifient dans
[`.github/release-notes-template.md`](.github/release-notes-template.md).

## Avertissement

Safe Invest est un **jeu éducatif**. L'argent est fictif, aucune transaction réelle n'est
jamais passée, et rien dans cette application ne constitue un conseil en investissement.
