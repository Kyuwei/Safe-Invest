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
que cet objectif réclame encore. Quand l'objectif tombe — atteint, ou échu — la partie se
fige et laisse un **bilan** : le résultat, la trajectoire, le meilleur et le pire trade, et
ce que ce rythme vaudrait tenu sur un an. C'est souvent la ligne la plus instructive de
toute la partie.

## Installer

Téléchargez **`safe-invest.exe`** depuis la
[dernière version](https://github.com/Kyuwei/Safe-Invest/releases/latest) et
double-cliquez. Un seul fichier, une dizaine de mégaoctets, rien à installer.

Windows 10 (version 2004 ou plus récente) ou Windows 11. L'application s'appuie sur
*Microsoft Edge WebView2*, présent d'origine sur Windows 11 et installé avec Edge sur
Windows 10. S'il manque, l'application le dit et donne le lien.

Vos parties sont dans `%LOCALAPPDATA%\SafeInvest\data`. Pour désinstaller : supprimez
le fichier, et le dossier `%LOCALAPPDATA%\SafeInvest` si vous ne voulez rien garder.

En cas de doute :

```
safe-invest.exe doctor
```

affiche où sont vos données, si le moteur web est présent, quelles sources de cours sont
configurées et où se trouve le journal de diagnostic.

Quand quelque chose se passe mal, ce journal est ce qu'il faut envoyer : **Paramètres →
Journal → Exporter le journal** en dépose une copie sur le Bureau. Il ne contient ni clé
d'API ni jeton — ils sont masqués avant écriture — et rien ne l'envoie à votre place.

Si la fenêtre ne peut pas s'ouvrir du tout — WebView2 absent, dossier de données
inaccessible —, une boîte de message le dit et donne la raison, même lancée d'un
double-clic.

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
| `crates/mcp` | Les seize outils MCP, une coquille sur `service` |
| `crates/app` | L'exécutable : la fenêtre Tauri, et le serveur MCP en sous-commande |

Le point important : **la fenêtre et l'IA appellent les mêmes fonctions**. Un ordre passé
à la souris et un ordre passé par une IA suivent les mêmes règles, les mêmes frais et les
mêmes contrôles, parce qu'il n'existe qu'un seul chemin vers le moteur. Et une partie
appartient à qui la joue : le moteur refuse un ordre de l'IA dans une partie humaine, et
un ordre de la fenêtre dans une partie IA.

Un seul fichier fait les deux :

```
safe-invest.exe             ouvre la fenêtre
safe-invest.exe mcp         parle le protocole MCP sur l'entrée et la sortie standard
safe-invest.exe mcp --http  sert le même MCP sur 127.0.0.1:9800
safe-invest.exe doctor      affiche un diagnostic
```

Les deux modes lisent et écrivent le même dossier de parties. L'application le surveille :
quand l'IA agit dans son processus, le tableau de bord se met à jour dans la seconde.
Chacun désigne sa partie — la fenêtre celle qu'elle affiche, chaque connexion MCP la
sienne —, si bien qu'ouvrir une partie d'un côté ne déplace jamais l'autre.

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

Le serveur expose seize outils : créer une partie, chercher un actif, lire les cours et
l'historique, acheter, vendre, suivre l'objectif, terminer et lire le bilan. Chacun
déclare s'il ne fait que lire ou s'il agit, et un refus revient comme un résultat que le
modèle lit — la raison et un conseil — plutôt que comme une erreur de protocole.

Si votre client préfère une adresse à un chemin d'exécutable, les Paramètres ouvrent le
même serveur sur un port de bouclage — `http://127.0.0.1:9800/mcp` par défaut, derrière un
jeton, refusé à toute origine qui n'est pas la machine elle-même.

En partie IA, `buy` et `sell` **refusent** un ordre sans justification. C'est délibéré :
tout l'intérêt du mode IA tient à ce que l'historique se lise comme une suite de décisions
expliquées. Une IA ne passe d'ordres que dans une partie IA ; une partie humaine, elle
peut seulement la lire.

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
cargo test --workspace          # plus de 200 tests, sans réseau
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
refuse un ordre qu'elle ne justifie pas — ou qu'elle passerait dans la partie d'une
personne —, un actif non coté signalé plutôt que valorisé à zéro et jamais figé dans un
résultat, et deux cents écritures concurrentes qui arrivent toutes. Des tests lancent le
vrai binaire et jouent une partie entière par-dessus les tuyaux MCP, à deux clients à la
fois.

### Publier une version

D'abord, la version : elle est écrite dans `Cargo.toml` (section `[workspace.package]` et
les cinq lignes `safe-invest-*` de `[workspace.dependencies]`) et dans
`crates/app/tauri.conf.json`. Changez-la aux deux endroits, laissez `cargo` mettre le
`Cargo.lock` à jour (`cargo update --workspace`), et poussez sur `main`. La CI refuse un
commit où les deux fichiers ne sont pas d'accord.

Ensuite, la publication :

```bash
git tag v0.3.0 && git push origin v0.3.0
```

ou, depuis l'onglet **Actions** de GitHub, lancer **Release** à la main en saisissant la
même version. Le workflow vérifie que l'étiquette correspond à la version du `Cargo.toml`,
compile, contrôle que l'exécutable démarre, puis publie `safe-invest.exe` et son
empreinte SHA-256. Les notes se modifient dans
[`.github/release-notes-template.md`](.github/release-notes-template.md).

## Avertissement

Safe Invest est un **jeu éducatif**. L'argent est fictif, aucune transaction réelle n'est
jamais passée, et rien dans cette application ne constitue un conseil en investissement.
