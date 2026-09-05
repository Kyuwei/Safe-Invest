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

**Les messages d'erreur ne recopient pas l'URL.** Une URL Finnhub contient la clé d'API ;
un message d'erreur finit dans un journal ou dans une bulle à l'écran. Les erreurs de
transport disent « connexion impossible » ou « délai dépassé », rien de plus. Un test
vérifie qu'une clé ne peut pas apparaître dans un message.

## Les clés d'API

**Chiffrées au repos.** Sous Windows, une clé est scellée par DPAPI sous le compte
utilisateur courant, avec une entropie propre à l'application : un autre compte de la
même machine ne peut pas la lire, même en ayant le fichier, et un autre programme ne peut
pas substituer un blob qu'il aurait scellé lui-même.

**Jamais réaffichées.** Il n'existe aucune commande pour relire une clé enregistrée.
L'interface montre « enregistrée » et rien d'autre. Réafficher un secret n'a aucune
utilité et crée une façon de le lire par-dessus une épaule.

**Hors Windows**, il n'y a pas d'équivalent à DPAPI. La valeur est alors stockée telle
quelle, dans un dossier limité à son propriétaire (`0700`), et **marquée comme étant en
clair** dans le fichier : les deux cas ne peuvent pas être confondus.

## La fenêtre

**Politique de sécurité de contenu stricte.** La page ne peut charger de script,
de style ou d'image que depuis elle-même. Pas de `unsafe-eval`, pas de source distante,
`object-src 'none'`, `frame-ancestors 'none'`.

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
`safe-invest-platform`** : le scellement DPAPI d'une clé, et l'attachement à la console
qui permet à un exécutable fenêtré de répondre à `--version` dans un terminal. Chacune
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

## Les fichiers

**Écriture atomique.** Une sauvegarde est écrite dans un fichier temporaire voisin,
synchronisée sur le disque, puis renommée par-dessus la cible. Un lecteur voit l'ancien
contenu ou le nouveau, jamais un mélange tronqué.

**Verrou entre processus.** La fenêtre et le serveur MCP écrivent le même dossier. Chaque
cycle lire-modifier-écrire tient un verrou du système d'exploitation pour toute sa durée.
Un test lance deux cents modifications concurrentes et vérifie qu'aucune ne se perd. Le
verrou est un fichier plutôt qu'un mutex nommé : le noyau le libère même si le processus
est tué en pleine écriture, donc un verrou oublié ne peut pas bloquer l'application.

**Un fichier corrompu ne bloque rien.** Une partie illisible est ignorée et signalée dans
le journal ; les autres restent accessibles. Un fichier de réglages illisible retombe sur
les valeurs par défaut.

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
