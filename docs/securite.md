# Sécurité

Safe Invest ne manipule pas d'argent réel, mais il lit des données venues d'Internet,
garde des clés d'API et fait tourner un moteur web. Voici ce qui est protégé, et comment.

## Le principe

Une seule règle explique la plupart des choix : **ce qui vient de l'extérieur est une
donnée, jamais une instruction.** Un cours, un nom d'actif, une page web, une réponse
d'API — rien de tout cela ne doit pouvoir devenir du code, ni faire paniquer le
programme, ni épuiser sa mémoire.

## Le réseau

**HTTPS, vérifié à chaque saut.** Toute requête est refusée si son URL n'est pas en
HTTPS, et chaque redirection est contrôlée à nouveau — une chaîne de redirections est un
bon moyen de sortir d'un canal chiffré. Une seule exception, documentée dans le code :
`http://127.0.0.1`, pour que la suite de tests puisse servir des réponses enregistrées
sans certificat. Ce trafic ne quitte jamais la machine.

**rustls, jamais OpenSSL.** Pas de bibliothèque TLS système à maintenir à jour, et le
même chemin de code sur Windows que sur le serveur d'intégration. Le fournisseur
cryptographique est `ring`.

**Les réponses sont plafonnées.** Une réponse HTTP est lue avec un plafond de 4 Mio,
vérifié pendant la lecture et pas seulement d'après l'en-tête annoncé. Un point d'accès
qui se mettrait à répondre par gigaoctets — panne ou malveillance — ne peut pas épuiser
la mémoire de l'application.

**Délais courts.** Cinq secondes pour établir la connexion, douze pour la réponse
complète. Une source lente est une source qu'on abandonne pour la suivante.

**Les clés ne voyagent pas dans l'URL.** Finnhub reçoit la sienne dans l'en-tête
`X-Finnhub-Token` : une URL est la partie d'une requête que les proxys et les journaux
recopient partout. Et les messages d'erreur ne recopient pas l'URL de toute façon : un
message finit dans un journal ou dans une bulle à l'écran, et les erreurs de transport
disent « connexion impossible » ou « délai dépassé », rien de plus. Un test vérifie
qu'une clé ne peut pas apparaître dans un message.

**Ce qui vient d'une IA est vérifié avant de partir.** Un symbole ou une devise saisis
par une IA finissent dans l'URL de chaque source interrogée. Un symbole doit avoir la
forme d'un ticker — lettres, chiffres, `. - _ = ^`, 32 caractères au plus, jamais `..` —
et une devise trois lettres ; tout est encodé à la sortie. Un nom de joueur et une
justification sont ramenés sur une ligne, sans caractère de contrôle, et bornés.

## Le port MCP, quand il est ouvert

L'application n'écoute sur rien tant que personne ne le demande. Le serveur MCP parle
par l'entrée et la sortie standard, et c'est le mode par défaut, précisément parce qu'il
n'ouvre aucun port. Le port local est une case à cocher, jamais un réglage initial :
ouvrir un port est une décision sur une machine, et ce n'est pas à un programme de la
prendre à la place de quelqu'un.

Une fois ouvert, quatre verrous indépendants le gardent, chacun suffisant seul :

**L'écoute est liée à `127.0.0.1`**, pas à `0.0.0.0`. L'adresse n'est pas configurable —
c'est le premier verrou, et le rendre réglable reviendrait à offrir un moyen de le
retirer. Rien venu du réseau ne peut ouvrir la connexion.

**Un jeton est exigé à chaque requête.** Deux UUID v4 en hexadécimal, soit 244 bits tirés
de la même source d'entropie qu'une clé. Il est scellé par DPAPI comme les clés d'API, il
n'apparaît ni dans les journaux ni dans le diagnostic — qui dit seulement s'il existe —
et la comparaison est faite en temps constant. « Régénérer le jeton » invalide l'ancien
immédiatement : chaque connexion appartient au serveur qui l'a acceptée et se ferme avec
lui, donc une connexion déjà ouverte — un flux d'événements, une connexion maintenue —
ne survit pas au changement de jeton, ni à la fermeture du port. Un test garde une
connexion ouverte pendant l'arrêt et vérifie qu'elle tombe.

**Un client lent ne bloque rien.** Un client a vingt secondes pour envoyer les en-têtes
de sa requête, et un `accept` qui échoue n'emballe pas le processeur.

**L'en-tête `Host` doit nommer le bouclage.** C'est ce qui ferme le réattachement DNS :
une page qui ferait pointer `evil.example` sur 127.0.0.1 nous atteindrait en même origine,
CORS hors-jeu, avec un `Host: evil.example` que ce contrôle rejette.

**L'`Origin`, quand il y en a une, doit être le bouclage.** Un client natif n'en envoie
pas et passe ; un navigateur en envoie toujours une et se fait renvoyer. Une page web n'a
pas besoin de lire la réponse pour faire du dégât : poster suffit à passer un ordre.

Deux détails qui comptent autant que les quatre verrous :

**Aucune réponse ne porte d'en-tête CORS**, ni sur un succès, ni sur un refus, et une
requête `OPTIONS` n'obtient jamais de réponse favorable. Un `Access-Control-Allow-Origin`
serait la seule ligne capable de défaire tout le reste, donc un test vérifie qu'aucune
réponse n'en porte. C'est aussi pour cela que le serveur n'accepte du JSON qu'en
`application/json` : c'est un type qu'un navigateur ne peut pas envoyer d'une autre
origine sans demander d'abord la permission — permission qui n'est jamais accordée.

**Seul `/mcp` existe.** Tout autre chemin renvoie 404 avant d'atteindre quoi que ce soit,
et seules les méthodes du transport sont acceptées.

## Les clés d'API

**Chiffrées au repos.** Sous Windows, une clé est scellée par DPAPI sous le compte
utilisateur courant, avec une entropie propre à l'application : un autre compte de la
même machine ne peut pas la lire, même en ayant le fichier, et un autre programme ne peut
pas substituer un blob qu'il aurait scellé lui-même.

**Jamais réaffichées, jamais confiées à la page.** Il n'existe aucune commande pour relire
une clé enregistrée. L'interface montre « enregistrée » et rien d'autre. L'écran des
paramètres ne reçoit même pas la forme scellée : il lit une vue des préférences sans
aucun secret, et n'envoie en retour que ce qui a changé — un changement qui contiendrait
un champ secret est refusé. Ce qu'une page n'a jamais eu, elle ne peut pas le réécrire.

**Sans fenêtre de dialogue.** DPAPI est appelé avec `CRYPTPROTECT_UI_FORBIDDEN` : le
serveur MCP n'a aucune fenêtre où afficher une demande, et une demande qu'il ne pourrait
pas montrer bloquerait l'appel.

**Hors Windows**, il n'y a pas d'équivalent à DPAPI. La valeur est alors stockée telle
quelle, dans un dossier limité à son propriétaire (`0700`), et **marquée comme étant en
clair** dans le fichier : les deux cas ne peuvent pas être confondus.

## La fenêtre

**Politique de sécurité de contenu stricte.** La page ne peut charger de script,
de style ou d'image que depuis elle-même. Pas de `unsafe-eval`, pas de source distante,
`object-src 'none'`, `frame-ancestors 'none'`.

**Chaque face désigne sa partie.** La fenêtre nomme la partie affichée à chaque appel, et
chaque connexion MCP garde la sienne. Le moteur refuse un ordre venu d'un autre joueur
que celui à qui appartient la partie : une IA ne peut pas trader le portefeuille d'une
personne, un clic ne peut pas se glisser dans l'historique d'une IA.

**Aucune permission de plateforme.** Le fichier de capacités n'accorde à la fenêtre que
les commandes de cette application, plus l'ouverture d'un dossier dans l'explorateur.
Pas de système de fichiers, pas de shell, pas de requête HTTP arbitraire depuis la page :
tout passe par des commandes Rust nommées.

**Rien n'est injecté en HTML.** Le code de l'interface construit ses éléments et pose le
texte par `textContent`. Un nom d'actif vient d'une API de marché ; une page qui colle du
texte distant dans du balisage est à une mauvaise réponse d'exécuter ce texte. La
fonction qui construit les éléments refuse explicitement le HTML brut.

## Le code

**Tout le code système au même endroit.** Le lint `unsafe_code` est actif sur tout
l'espace de travail, et **chaque ligne `unsafe` du projet est dans le crate
`safe-invest-platform`** : le scellement DPAPI d'une clé, l'attachement à la console
qui permet à un exécutable fenêtré de répondre à `--version` dans un terminal, et la
boîte de message qui annonce une erreur de démarrage quand il n'y a pas de terminal. Chacune
porte une autorisation nommée et un commentaire `SAFETY` qui dit pourquoi l'appel est
correct.

Ce regroupement a une seconde vertu, pratique celle-là. Ce crate ne dépend que de
`windows-sys`, donc il se vérifie pour la cible Windows depuis une machine Linux —
tout le reste de l'espace de travail tire `ring`, dont le script de compilation ne sait
pas viser MSVC en compilation croisée. La CI fait tourner cette vérification à chaque
poussée. Elle a déjà attrapé trois erreurs de signature Win32 qui, sans elle, ne seraient
apparues qu'après plusieurs minutes de compilation Windows.

**Ni `unwrap`, ni `expect`, ni `panic`, ni indexation de tranche** dans le code hors
tests : ces lints sont actifs pour tout l'espace de travail. Un cours absurde ressort
comme un ordre refusé, pas comme un plantage.

**L'arithmétique monétaire est vérifiée.** Toute opération sur les montants passe par des
fonctions qui renvoient une erreur en cas de dépassement, plutôt que de paniquer ou de
tronquer.

**Écrire sur une sortie impossible n'est pas une panique.** `println!` panique quand
l'écriture échoue, et un exécutable en sous-système « windows » lancé sans terminal n'a
pas de sortie standard. Sous `panic = "abort"`, cela donnait un code d'erreur muet.
L'affichage passe maintenant par une fonction qui ignore l'échec, et des tests lancent le
binaire avec sa sortie redirigée vers `/dev/full` — qui fait échouer toute écriture — pour
vérifier que le code de retour reste juste.

## Le journal

Un journal existe pour être envoyé à quelqu'un. C'est ce qui décide de tout le reste.

**Aucun secret n'y entre.** Chaque valeur que le programme traite comme un secret — une
clé d'API dès qu'elle est déchiffrée, le jeton MCP dès qu'il est créé ou relu — est
enregistrée auprès du journal, qui la remplace par `[secret masqué]` au moment de
l'écriture. La protection ne dépend donc pas de la prudence de chaque appel à `tracing` :
elle est appliquée en dernier, sur le texte qui part vers le fichier. Un test le vérifie,
et deux autres vérifient qu'une clé lue et un jeton créé sont bien connus du journal.

**Il est borné.** Un fichier courant d'un mégaoctet, un fichier précédent, et rien de
plus : une rotation remplace le second par le premier. Un journal qui grossit sans fin
est un défaut, pas une fonctionnalité.

**Il ne part nulle part tout seul.** Rien ne l'envoie : l'export écrit un fichier sur le
Bureau et s'arrête là. Ce que la personne en fait ensuite lui appartient.

**Il ne peut pas faire échouer l'application.** Un disque plein ou un dossier en lecture
seule rend le journal indisponible, jamais le lancement impossible : les écritures sont
silencieusement abandonnées et un message le dit au démarrage. Une rotation qui échoue
laisse le journal écrire là où il était, et un processus qui trouve le fichier déjà
tourné par un autre le suit au lieu de tourner une seconde fois.

**Il garde la trace d'une erreur interne.** La version publiée s'arrête net sur une
erreur interne (`panic = "abort"`). Avant, elle écrit le message et l'endroit dans le
journal.

## Les fichiers

**Écriture atomique.** Une sauvegarde est écrite dans un fichier temporaire voisin,
synchronisée sur le disque, puis renommée par-dessus la cible. Un lecteur voit l'ancien
contenu ou le nouveau, jamais un mélange tronqué. Sous Windows, un renommage refusé parce
qu'un antivirus ou l'indexeur examine le fichier est retenté quelques millisecondes plus
tard, plutôt que de perdre l'ordre. Les réglages suivent la même règle.

**Verrou entre processus.** La fenêtre et le serveur MCP écrivent le même dossier. Chaque
cycle lire-modifier-écrire — une partie comme les réglages — tient un verrou du système
d'exploitation pour toute sa durée : une case cochée dans la fenêtre ne peut plus effacer
le jeton que le serveur vient de créer.
Un test lance deux cents modifications concurrentes et vérifie qu'aucune ne se perd. Le
verrou est un fichier plutôt qu'un mutex nommé : le noyau le libère même si le processus
est tué en pleine écriture, donc un verrou oublié ne peut pas bloquer l'application.

**Un fichier corrompu ne bloque rien.** Une partie illisible est ignorée et signalée dans
le journal ; les autres restent accessibles. Un fichier de réglages illisible retombe sur
les valeurs par défaut, et une valeur hors limites — un port sous 1024, un
rafraîchissement de zéro seconde — est ramenée dans les bornes.

## Les dépendances

La politique est dans [`deny.toml`](../deny.toml) et la CI l'applique à chaque poussée :

- `cargo audit` — vulnérabilités connues ;
- `cargo deny check` — licences autorisées, sources autorisées (crates.io seulement),
  versions génériques interdites, avis de sécurité.

Les avis ouverts sont tous de type « non maintenu » ou « peu sûr », jamais des
vulnérabilités, et **chacun porte une justification écrite**. Dix d'entre eux concernent
les liaisons GTK3 utilisées uniquement par la version Linux : ils n'existent pas dans le
graphe de dépendances Windows, ce qui se vérifie par
`cargo tree --target x86_64-pc-windows-msvc`.

L'interface n'a **aucune dépendance npm**. C'est du HTML, du CSS et des modules
JavaScript écrits à la main : pour la partie qui affiche des données venues du réseau, la
chaîne d'approvisionnement est vide.

## Ce que ce programme ne fait pas

Il ne passe aucun ordre réel. Il ne demande aucun identifiant bancaire. Il n'envoie
aucune donnée personnelle nulle part : les seules requêtes sortantes vont aux API de
cours, et elles ne transportent qu'un symbole boursier.

## Signaler un problème

Ouvrez une [issue](https://github.com/Kyuwei/Safe-Invest/issues). S'il s'agit d'une
faille exploitable, décrivez-la sans publier de code d'exploitation.
