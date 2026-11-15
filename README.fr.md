# Assemblages bois pour SolidCraft

*[English version](README.md)*

Un plugin pour [SolidCraft](https://github.com/tangency-labs/solidcraft) qui réalise des
assemblages de menuiserie. Il apporte deux commandes : **Tenon et mortaise**, entre deux pièces,
et **Rainure**, pour une rainure, une entaille ou une feuillure dans une pièce.

Chaque commande ajoute des fonctions « boîte » ordinaires dans la timeline, en une seule étape
d'annulation. Un fichier créé avec le plugin s'ouvre et se reconstruit sans lui.

## Installer

1. Téléchargez `wood-joinery-fr.zip` depuis la
   [dernière version publiée](https://github.com/WilliamHJKL/solidcraft-wood-joinery/releases/latest).
   Pour les textes en anglais, téléchargez `wood-joinery.zip`.
   Ne prenez pas les archives « Source code » : SolidCraft ne peut pas les installer.
   Vous pouvez aussi construire les paquets vous-même (voir [Construire](#construire)) ; ils sont
   alors dans `dist/`.
2. Dans SolidCraft, ouvrez Preferences › Plugins › Install plugin… et choisissez le fichier.
3. Les deux commandes sont alors dans le menu Plugins, et dans le groupe Plugins de l'onglet Model.

Le plugin appelle deux commandes de SolidCraft, `inspect.measure` et `solid.box`. Il ne demande
aucun accès aux fichiers, à l'horloge ou au réseau.

N'installez qu'une seule langue : les deux paquets sont le même plugin.

## Tenon et mortaise

![Un tenon à renfort dans un pied, le pied affiché en transparence](docs/mortise-tenon.png)

Saisissez le nom de la traverse et celui du montant, tels que le navigateur les affiche. Le
plugin ajoute le tenon au bout de la traverse et creuse la mortaise correspondante dans le
montant. La boîte englobante de la traverse comprend alors ses tenons : une fiche de débit lit la
vraie longueur de la pièce brute.

Le bout de la traverse doit toucher une face du montant avant de lancer la commande.

| Assemblage | Ce qu'il réalise |
|---|---|
| Classique (par défaut) | Une mortaise borgne. Le tenon a un épaulement sur chaque chant. |
| À renfort | Une mortaise borgne avec un renfort court, sur le chant de la traverse le plus proche du bout du montant. |
| Traversant | Un tenon qui traverse tout le montant. |
| Enfourchement | Une fente ouverte au bout du montant. La languette fait toute la largeur de la traverse. |

Toutes les cotes sont facultatives, en millimètres. Un champ vide suit les usages de la
menuiserie :

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Épaisseur du tenon | `thickness` | Un tiers de l'épaisseur de la traverse |
| Longueur du tenon | `length` | Deux tiers du montant, arrondis à 5 mm. Tout le montant pour Traversant et Enfourchement |
| Épaulement | `shoulder` | La largeur de la joue, au plus un cinquième de la largeur de la traverse. Aucun pour Enfourchement |
| Largeur du renfort | `haunch_width` | Un cinquième de la largeur de la traverse |
| Profondeur du renfort | `haunch_depth` | L'épaisseur du tenon |
| Réserve de colle | `glue_relief` | 2 mm sous un tenon borgne |
| Jeu | `clearance` | Aucun |
| Décalage | `offset` | Le tenon est centré dans l'épaisseur de la traverse |

Exemple : une traverse de 18 × 45 mm contre un montant de 35 × 35 mm reçoit un tenon de
6 × 33 × 25 mm, dans une mortaise de 27 mm de profondeur.

Le plugin refuse un assemblage impossible et dit pourquoi : un tenon borgne qui traverserait le
montant, un tenon aussi épais que la traverse, une traverse qui ne touche pas le montant, un
enfourchement loin du bout du montant.

## Rainure

![Un pied avec deux rainures arrêtées](docs/groove.png)

Saisissez le nom de la pièce et choisissez une face. La rainure suit le grand côté de cette
face, d'un bout à l'autre.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Face | `face` | Obligatoire : `+x`, `-x`, `+y`, `-y`, `+z` ou `-z` |
| Largeur | `width` | 6 mm |
| Profondeur | `depth` | 8 mm |
| Décalage | `offset` | Centrée. Sinon, la distance entre le bord de la face le plus proche de l'origine et la rainure. Zéro donne une feuillure |
| Arrêt au début | `stop_start` | La rainure débouche. Sinon, cette longueur de bois reste au bout le plus proche de l'origine |
| Arrêt à la fin | `stop_end` | Même chose, à l'autre bout |

Sur une face plus large que longue, la même commande réalise une entaille en travers.

## Limites de cette version

- Les pièces doivent être alignées sur les axes X, Y ou Z. Le plugin lit la boîte englobante de
  chaque corps : une pièce en biais n'est pas prise en charge. Les faces se désignent par leur
  axe, elles ne se cliquent pas.
- La profondeur du montant est celle de sa boîte englobante dans l'axe de l'assemblage. Un
  montant avec un pied ou un retour est donc mesuré trop profond : fixez alors vous-même la
  longueur du tenon.
- Les cotes sont des nombres. Elles ne suivent pas un paramètre : après un changement, annulez
  l'usinage et relancez la commande. Un assemblage qui se reconstruit seul attend les fonctions
  paramétriques par plugin, prévues pour une phase ultérieure du RFC des plugins de SolidCraft.
- Un tenon ne s'ajuste pas encore à une rainure déjà creusée dans la traverse ou le montant.
- Les messages d'erreur du plugin sont en anglais dans tous les paquets.

## Lancer depuis un script

Avec `solidcraft-cli`, inspectez le plugin, puis passez la valeur `approval` qu'il renvoie à
`plugin.run`. Les entrées sont `mortise-tenon` et `groove`.

```json
[{"command": "plugin.inspect", "params": {"path": "dist/wood-joinery"}}]
```

```json
[{"command": "plugin.run", "params": {
  "path": "dist/wood-joinery",
  "approval": "LA_VALEUR_APPROVAL_DE_INSPECT",
  "entry": "mortise-tenon",
  "params": {"rail": "Rail", "stile": "Stile", "variant": "haunched"}
}}]
```

```json
[{"command": "plugin.run", "params": {
  "path": "dist/wood-joinery",
  "approval": "LA_VALEUR_APPROVAL_DE_INSPECT",
  "entry": "groove",
  "params": {"body": "Leg", "face": "+x", "offset": 6, "stop_start": 80}
}}]
```

Les noms des paramètres et les valeurs (`haunched`, `+x`…) sont les mêmes dans toutes les
langues : seuls les textes affichés sont traduits.

## Construire

Il faut Rust avec la cible `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`)
et Python 3.

```sh
./build.sh
```

Le script écrit un paquet par langue : `dist/wood-joinery.zip` (anglais) et
`dist/wood-joinery-fr.zip` (français).

## Tester

```sh
cargo test                                   # le calcul de chaque usinage
tests/check.py /chemin/vers/solidcraft-cli   # le plugin emballé, dans un vrai SolidCraft
```

`tests/check.py` lance chaque assemblage et chaque rainure, compare les volumes mesurés à des
valeurs calculées à la main, et vérifie l'annulation, les refus, la réouverture d'un fichier
sans le plugin, et chaque paquet traduit.

## Traduire

SolidCraft affiche les textes d'un plugin tels que son manifeste les donne, dans une seule
langue. Les textes anglais sont dans `plugin/plugin.json`. Une traduction tient en un fichier,
`plugin/locales/<langue>.json`, qui associe chaque texte anglais à sa traduction ;
`plugin/locales/fr.json` sert de modèle.

`./build.sh` écrit alors `dist/wood-joinery-<langue>.zip`. La construction échoue si un texte
n'a pas de traduction, ou si une traduction ne sert plus : une langue ne peut pas prendre de
retard.

## Licence

MIT OR Apache-2.0, comme SolidCraft. `wit/plugin.wit` est l'interface de plugin de SolidCraft
(`craft:plugin@0.1.0`), copiée de son dépôt sous la même licence.
