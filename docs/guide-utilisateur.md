# Guide de l'utilisateur

Safe Invest est un **bac à sable** : l'argent est fictif, les cours sont réels. Rien de ce
que vous ferez ici ne peut vous coûter quoi que ce soit.

## 1. Le menu d'accueil

Trois portes d'entrée :

- **Nouvelle partie** — choisir qui joue et avec combien
- **Réglages** — les sources de cours, les clés API, l'affichage
- **Comment ça marche ?** — l'essentiel en deux minutes, à lire avant la première partie

En dessous, les parties déjà commencées. Un clic les rouvre là où vous les aviez laissées.

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

## 3. Le tableau de bord

**La valeur totale** en gros. Juste en dessous, le gain ou la perte, en euros et en
pourcentage, en vert si le portefeuille vaut plus que le capital de départ et en rouge
sinon — avec un « + » explicite sur les gains, pour que la couleur ne soit jamais le seul
signal.

À droite, trois chiffres :

- **Argent disponible** — ce qui n'est pas investi et peut servir à acheter
- **Investi** — ce que valent aujourd'hui vos positions
- **Gains déjà encaissés** — le résultat des ventes déjà faites, définitivement acquis

**La courbe** apparaît sous les chiffres dès qu'il y a de quoi tracer une ligne. Elle est
enregistrée au fil des relevés — un point tous les quarts d'heure, un mois d'historique —
et non reconstruite après coup : une courbe reconstruite devrait deviner des cours que
personne n'a notés.

**L'anneau d'objectif** montre le chemin parcouru depuis le capital de départ, le temps
restant, et le rendement annuel encore nécessaire.

**Les lignes du portefeuille** : une carte par actif, avec la quantité, le prix d'achat
moyen, le cours actuel et la plus ou moins-value. Le liseré de couleur à gauche dit d'un
coup d'œil si la ligne est en gain ou en perte, et un badge donne son poids dans le
portefeuille. Un cours simulé est signalé sous la carte.

En partie IA, une section supplémentaire liste **les dernières décisions de l'IA**, chacune
avec sa justification.

## 4. Le marché

La recherche accepte un nom (« bitcoin », « microsoft ») ou un symbole (« BTC », « MSFT »).
Sans rien taper, l'application propose une sélection d'actifs connus.

Chaque ligne montre le cours, la variation sur 24 heures en vert ou en rouge, et la
mention « simulé » quand le chiffre ne vient pas d'une vraie source.

Le bouton **Détails** ouvre la fiche de l'actif : le cours, sa variation du jour, la
courbe des trente derniers jours avec le mouvement sur la période, ce que vous en détenez
déjà, et **une phrase sur ce qu'est ce type d'actif**. C'est le moment où quelqu'un est le
plus disposé à la lire : juste avant d'acheter.

De là, le bouton **Acheter** ouvre une fiche qui rappelle le cours et propose deux façons de
dimensionner l'ordre :

- **pour un montant** — « 3 000 € » ; la fiche répond aussitôt *« soit environ
  0,049 BTC au cours actuel »*, ce qui rend l'ordre concret ;
- **par quantité** — « 0,05 BTC » ; la fiche annonce ce que cela coûtera.

Depuis le tableau de bord, une position offre en plus **Vendre**, avec un troisième
choix : **Tout vendre**, qui solde la ligne entière au cours du moment.

L'estimation affichée est une prévision. Le calcul qui compte est fait par le programme
au moment de l'ordre, en décimal exact, et c'est lui qui apparaît dans l'historique.

## 5. L'historique

Chaque opération, de la plus récente à la plus ancienne : la date, l'actif, la quantité, le
prix unitaire, le total, les frais, et le résultat réalisé pour une vente.

Quand c'est une IA qui a joué, sa **justification** apparaît en italique sous l'opération.
C'est le vrai support pédagogique de l'application : une suite de décisions datées et
argumentées, qu'on peut relire et critiquer après coup.

## 6. Les réglages

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

## Rappel

L'argent est fictif, aucune transaction réelle n'est jamais passée, et rien dans cette
application ne constitue un conseil en investissement.
