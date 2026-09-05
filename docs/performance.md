# Ce que Safe Invest coûte

Mesuré, pas estimé. Le script qui produit ces chiffres est
[`scripts/profile.sh`](../scripts/profile.sh) ; relancez-le après toute
modification qui pourrait coûter quelque chose.

```bash
cargo build --release
./scripts/profile.sh
```

## Relevé de référence

Machine de test : Ubuntu 24.04, 4 cœurs, sans accélération graphique, build
`--release` du 5 septembre 2026.

| Mesure | Valeur |
|---|---|
| Exécutable (Linux) | **9,6 Mo** — un seul fichier |
| Exécutable (Windows, mesuré en CI) | **7,6 Mo** |
| dont interface embarquée | 80 Ko |
| Démarrage `--version` | 24 ms (moyenne sur 10) |
| Démarrage `doctor` | 22 ms |
| Appel MCP `get_quotes` (3 symboles, en cache) | **0,18 ms** |
| Serveur MCP au repos | 38 Mo |
| Fenêtre — processus Safe Invest | **36 Mo alloués** (170 Mo RSS) |
| Fenêtre — moteur web | 195 Mo alloués (366 Mo RSS) |

À titre de comparaison, la version 0.1 (.NET 10 + WinUI 3) livrait 95 Mo
d'archive et ne démarrait pas.

## Lire ces chiffres

**RSS anonyme contre RSS total.** Le RSS total compte les pages de
bibliothèques partagées dans *chaque* processus qui les projette : GTK, WebKit
et Mesa apparaissent ainsi trois fois. Le RSS anonyme est la mémoire que le
processus a réellement demandée. C'est celui qu'il faut regarder pour juger le
code de Safe Invest — 36 Mo — et c'est celui qui bouge quand on introduit une
fuite.

**Le moteur web domine, et il n'est pas à nous.** Les 195 Mo du processus de
rendu sont ceux de WebKitGTK en rasterisation logicielle, sur une surface de
1280 × 900 sans GPU. Sur Windows le moteur est WebView2, dont l'empreinte est
différente et souvent partagée avec d'autres applications qui l'utilisent. **Ce
chiffre-là doit être remesuré sur Windows** avant d'en conclure quoi que ce
soit ; le gestionnaire des tâches suffit.

**0,18 ms par appel d'outil** correspond à un cours servi depuis le cache
mémoire. Un appel qui sort réellement sur le réseau se compte en centaines de
millisecondes, dominées par l'API distante.

## Ce qui a été fait pour ces chiffres

**Profil de compilation.** `lto = "fat"`, `codegen-units = 1`,
`panic = "abort"`, `opt-level = "s"` et `strip = "symbols"`. Le LTO complet et
l'unité de génération unique laissent l'éditeur de liens supprimer tout ce qui
est inatteignable ; sans `strip`, la table des symboles doublait la taille du
fichier. `opt-level = "s"` plutôt que `3` parce que rien ici n'est limité par
le calcul : le programme attend le réseau.

Le LTO complet a été comparé au LTO « thin », qui compile bien plus vite :
11,6 Mo contre 9,97 Mo, soit **16 % de plus**. Pour un livrable dont tout
l'intérêt est d'être un seul petit fichier, les six minutes de compilation
supplémentaires en valent la peine — elles ne sont payées qu'en CI.

**Un exécutable unique, pas d'installateur.** L'interface est un dossier de
fichiers statiques compilé dans le binaire. Pas de runtime à déployer à côté,
pas de dossier `_files`, rien à désinstaller.

**Deux fils asynchrones, pas un par cœur.** Tokio ouvre par défaut autant de
travailleurs qu'il y a de cœurs. Le travail asynchrone de cette application,
c'est quelques requêtes HTTP par minute ; deux travailleurs suffisent, et sur
une machine à seize cœurs cela évite de garder en mémoire quatorze piles de
fils qui ne servent à rien. *Mesure honnête : sur la machine de test à quatre
cœurs, la différence n'est pas visible. Le gain est un plafond, pas une
économie constatée ici.*

**Les cours vont par six, pas un par un.** Yahoo, Finnhub et le lecteur de
pages cotent symbole par symbole. En série, un tableau de vingt lignes
coûtait vingt allers-retours réseau à la file. Ils partent maintenant par
six — largement dans le budget annoncé par chaque source — et une source
qui répond pour certains symboles et pas pour d'autres rend ce qu'elle a
au lieu de tout jeter.

**Un cache de cours à durée de vie.** Sans lui, un tableau de bord de huit
lignes déclencherait huit requêtes par minute et par source, et brûlerait un
quota gratuit en une après-midi. Avec lui, la seconde consultation coûte 0,18 ms.

**Un limiteur à jetons par source.** Vingt lignes, pas un arbre de dépendances.
Il empêche l'application de dépasser le budget annoncé par chaque API.

**Une taille de réponse plafonnée.** Les réponses HTTP sont lues avec un
plafond de 4 Mio, vérifié pendant la lecture. Un point d'accès qui se mettrait
à répondre par gigaoctets — panne ou malveillance — ne peut pas épuiser la
mémoire de l'application.

## Chercher plus loin

Le profil `profiling` du `Cargo.toml` produit un binaire optimisé mais avec les
symboles et les tables de lignes que réclame un profileur :

```bash
cargo build --profile profiling
perf record -g ./target/profiling/safe-invest mcp --demo   # Linux
```

Sur Windows, l'analyseur de performances de Visual Studio ou
[Superluminal](https://superluminal.eu/) lisent le même binaire.
