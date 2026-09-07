# Guide de l'utilisateur

Safe Invest est un **bac à sable** : l'argent est fictif, les cours sont réels. Rien de ce
que vous ferez ici ne peut vous coûter quoi que ce soit.

## 1. Le menu d'accueil

L'écran est coupé en deux. À gauche, ce que fait l'application et le bouton **Comment ça
marche ?** — l'essentiel en deux minutes, à lire avant la première partie. À droite, le
formulaire de **nouvelle partie**, directement : il n'y a pas d'écran intermédiaire.

Sous le formulaire, les parties déjà commencées. Un clic les rouvre là où vous les aviez
laissées ; la croix à droite en supprime une, après confirmation.

Tout en bas, **Réglages** : les sources de cours, les clés API, l'accès IA et l'affichage.

## 2. Créer une partie

### Qui joue ?

**Une personne.** Vous cherchez les actifs, vous décidez, vous passez les ordres.

**Une IA.** C'est un assistant qui décide, à travers le serveur MCP (voir [mcp.md](mcp.md)).
L'application passe alors en mode observation : vous voyez ce que l'IA achète et vend, à
quel cours, et surtout **pourquoi**. C'est le mode le plus intéressant à regarder à
plusieurs : chaque décision est écrite, donc discutable.

### Combien ?

Trois montants d'un clic (1 000, 10 000, 100 000 €) ou n'importe quelle somme au clavier.
Un conseil : commencer petit rend les erreurs plus parlantes.

À côté, la devise et les **frais par opération** (0,1 % à 1 % est réaliste). Avec des
frais, on découvre vite qu'acheter et revendre sans arrêt coûte cher.

### L'objectif

Un montant à atteindre et une date limite. C'est la consigne donnée à une IA : les deux
champs apparaissent dès qu'on choisit « Une IA ».

Dès que les deux sont remplis, l'écran dit ce que l'objectif **exige vraiment** :

> Il faudrait environ +9 900,0 % par an — aucun placement réel ne tient ça, même une année.

Pour un objectif plus sage, la phrase change de ton :

> Il faudrait environ +7,2 % par an — c'est raisonnable, proche de ce que fait un marché
> actions sur longue durée.

C'est peut-être la leçon la plus utile de l'application, et elle arrive avant même d'avoir
joué le premier ordre.

## 3. Le portefeuille

Une fois la partie ouverte, une **barre latérale** tient l'écran : Portefeuille, Marché,
Historique, et plus bas Paramètres et le retour au menu. Un filet indigo marque la section
courante, et une pastille en bas rappelle qui joue — vous, ou une IA.

**La valeur totale** en gros. À côté, le gain ou la perte, en euros et en pourcentage, en
vert si le portefeuille vaut plus que le capital de départ et en rouge sinon — avec un
« + » explicite sur les gains, pour que la couleur ne soit jamais le seul signal.

**Quatre cartes** ensuite : ce qui est investi et sur combien de lignes, les liquidités et
la part du total qu'elles représentent, le gain depuis le départ, et la meilleure ligne du
portefeuille.

**La courbe** apparaît dès qu'il y a de quoi tracer une ligne. Elle est enregistrée au fil
des relevés — un point tous les quarts d'heure, un mois d'historique — et non reconstruite
après coup : une courbe reconstruite devrait deviner des cours que personne n'a notés.

**La répartition**, à côté, montre comment l'argent est réparti entre cryptos, actions et
ETF — **liquidités comprises**. Quelqu'un qui laisse neuf dixièmes de son argent en
liquidités a un portefeuille à neuf dixièmes en liquidités, et un graphique qui l'omet dit
le contraire de ce qu'il faut savoir.

**Le bandeau de mission** apparaît dès qu'un objectif est fixé : le montant visé, la date,
la barre d'avancement, les jours restants et le rythme encore nécessaire. Il s'affiche
aussi pour une partie humaine avec objectif — qui s'en fixe un mérite le même décompte.

**Le tableau des positions** donne, par ligne : l'actif, la quantité, le cours actuel, la
valeur, la variation sur 24 heures et la plus ou moins-value. Un cours simulé est signalé
sous le prix. Deux boutons : **Fiche** ouvre la page de l'actif, **Vendre** solde tout ou
partie de la ligne.

En partie IA, un **journal** s'intercale : chaque décision avec son heure, son montant et
sa justification, mis à jour en direct pendant que l'IA joue dans son processus.

## 4. Le marché

La recherche accepte un nom (« bitcoin », « microsoft ») ou un symbole (« BTC », « MSFT »).
Sans rien taper, l'application propose une sélection d'actifs connus.

Chaque ligne montre le cours, la variation sur 24 heures en vert ou en rouge, et la
mention « simulé » quand le chiffre ne vient pas d'une vraie source.

Le bouton **Fiche** ouvre la page de l'actif : le cours, sa variation du jour, la
courbe des trente derniers jours avec le mouvement sur la période, ce que vous en détenez
déjà, et **une phrase sur ce qu'est ce type d'actif**. C'est le moment où quelqu'un est le
plus disposé à la lire : juste avant d'acheter.

Le bouton **Acheter** — sur la fiche, ou directement sur la ligne du marché — ouvre une
boîte qui rappelle le cours et propose deux façons de dimensionner l'ordre :

- **pour un montant** — « 3 000 € » ; elle répond aussitôt *« soit environ 0,049 BTC au
  cours actuel »*, ce qui rend l'ordre concret ;
- **par quantité** — « 0,05 BTC » ; elle annonce ce que cela coûtera.

La fiche est aussi d'où l'on vend, et le tableau des positions y mène en un clic. À la
vente s'ajoute un troisième choix : **Tout vendre**, qui solde la ligne entière au cours du
moment.

Une ligne de portefeuille, elle, ne propose pas d'acheter : elle mène à la fiche. C'est
délibéré — la fiche montre le cours, le mois écoulé et ce qu'est ce type d'actif, et c'est
exactement ce qu'il faut avoir sous les yeux avant de remettre de l'argent sur une ligne
qu'on détient déjà.

L'estimation affichée est une prévision. Le calcul qui compte est fait par le programme
au moment de l'ordre, en décimal exact, et c'est lui qui apparaît dans l'historique.

## 5. L'historique

Un tableau, de l'opération la plus récente à la plus ancienne : la date, l'achat ou la
vente, l'actif, la quantité, le prix unitaire, le montant, et le résultat réalisé sous le
montant pour une vente. En tête, le nombre d'opérations et le volume échangé depuis la
première.

Un champ de recherche et trois filtres — Tout, Achats, Ventes — réduisent la liste sans
rien recalculer : ils cachent des lignes, ils n'en changent aucune.

Une colonne dit **qui** a passé l'ordre, vous ou l'IA, et la dernière porte sa
**justification**. C'est le vrai support pédagogique de l'application : une suite de
décisions datées et argumentées, qu'on peut relire et critiquer après coup.

## 6. La fin d'une partie

Une partie se termine de trois façons&nbsp;: l'objectif est atteint, la date limite passe,
ou vous cliquez sur **Terminer la partie**. Les deux premières sont automatiques — la
partie s'arrête à l'évaluation qui le constate, et pas plus tard.

**La valeur est figée à cet instant.** C'est le point important&nbsp;: le bilan raconte ce
qui s'est passé, il ne se recalcule pas au cours du jour. Une partie gagnée à 26 140 € le
restera, même si les cryptos qu'elle détenait ont fondu la semaine suivante. La courbe
s'arrête là aussi&nbsp;; elle ne continue pas après la fin de la partie.

Ensuite, plus aucun ordre n'est accepté, ni depuis la fenêtre ni par l'IA, et l'écran
**Bilan** prend la main&nbsp;:

- **le résultat**, comparé au capital de départ et à l'objectif s'il y en avait un&nbsp;;
- **la trajectoire** de toute la partie, avec l'objectif en pointillé quand la courbe l'a
  approché — s'il était hors d'atteinte, aucune ligne n'est tracée plutôt qu'une ligne
  collée au bord&nbsp;;
- **le meilleur et le pire trade**, et la part des ventes gagnantes. Cette part ne compte
  que les **ventes**&nbsp;: un achat n'a encore rien gagné ni perdu, et le compter
  diviserait le chiffre par deux pour une raison qui n'a rien à voir avec vos choix&nbsp;;
- **ce que le résultat vaut ramené à l'année.** C'est la ligne la plus utile de l'écran.
  +161 % en dix-huit jours est un très beau passage&nbsp;; le même rythme tenu un an
  n'existe nulle part, et le dire est tout l'intérêt d'un simulateur&nbsp;;
- **ce que la partie a montré** — quelques constats sur ce qui a été fait, cochés ou non.

Une partie terminée reste consultable&nbsp;: elle apparaît dans le menu avec la mention
« Terminée » et s'ouvre directement sur son bilan.

## 7. Les paramètres

Accessibles depuis la barre latérale pendant une partie, ou depuis le menu d'accueil.

**Sources de cours** — l'état de chaque source, avec une pastille verte quand elle a
répondu, rouge quand elle a échoué — avec la raison — et grise tant qu'elle n'a pas
servi. La liste est dans l'ordre d'essai.

**Mode démonstration** — des cours générés localement, sans réseau. Pratique en classe.
Ces cours sont signalés partout dans l'application.

**Clés API** — facultatives (voir [cles-api.md](cles-api.md)). Elles sont chiffrées avec
votre compte Windows et ne quittent jamais la machine.

**Palette adaptée au daltonisme** — remplace le vert et le rouge par un bleu et un orange.
Environ un homme sur douze distingue mal le vert du rouge ; sur un écran financier, c'est
le signal le plus important qui devient illisible.

**Rafraîchissement** — à quelle fréquence les cours sont relus.

**Accès IA — serveur MCP** — le bloc de configuration à coller dans votre client, déjà
rempli avec le chemin de *votre* exécutable, et la liste des outils qu'une IA se verra
confier. Le serveur parle par l'entrée et la sortie standard : il n'ouvre aucun port, donc
il n'y a ni adresse à exposer ni jeton à protéger.

**Ouvrir le dossier des parties** — le dossier des sauvegardes. C'est le même que lit le
serveur MCP : c'est ce qui permet à une IA de jouer la partie que votre fenêtre affiche.

## Quelques idées de séances

- **Le pari unique.** 10 000 € sur une seule crypto, et on regarde une semaine plus tard.
- **La diversification.** Deux parties en parallèle : l'une tout sur un actif, l'autre
  répartie sur six. On compare les secousses.
- **Le poids des frais.** Deux parties identiques, l'une à 0 %, l'autre à 1 %, en achetant
  et revendant souvent des deux côtés.
- **L'objectif impossible.** Se fixer un objectif absurde et lire ce que l'application
  répond avant même de commencer.
- **La partie IA commentée.** Donner un objectif à une IA, puis relire son historique
  ensemble et discuter chaque justification.
- **Le bilan qui dégonfle.** Terminer une partie qui a bien marché et lire à voix haute la
  phrase du bilan qui ramène le résultat à l'année. C'est le moment où « j'ai fait
  +40 % » devient une question plutôt qu'une conclusion.

## Rappel

L'argent est fictif, aucune transaction réelle n'est jamais passée, et rien dans cette
application ne constitue un conseil en investissement.
