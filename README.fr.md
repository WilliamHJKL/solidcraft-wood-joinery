# Assemblages bois pour SolidCraft

*[English version](README.md)*

Un plugin pour [SolidCraft](https://github.com/tangency-labs/solidcraft) qui réalise des
assemblages de menuiserie. Il apporte treize commandes : **Tenon et mortaise**, **Rainure** (et
aussi entaille et feuillure), **Mi-bois**, **Tourillons**, **Queues droites**, **Faux tenon**,
**Entaille d'étagère**, **Coupe d'onglet**, **Rainure et languette**, **Enture**,
**Queues d'aronde**, **Vis** et **Lamelles**.

Chaque commande ajoute des fonctions ordinaires dans la timeline (boîtes, cylindres, déplacements
et coupes), en une seule étape d'annulation. Un fichier créé avec le plugin s'ouvre et se
reconstruit sans lui.

## Installer

Il faut l'application de bureau SolidCraft, en version 0.46.0 ou plus récente. La version web
n'exécute aucun plugin. Avec SolidCraft 0.42 à 0.45, installez la
[version 0.10.1](https://github.com/WilliamHJKL/solidcraft-wood-joinery/releases/tag/v0.10.1) du
plugin : depuis la 0.11.0, les formulaires n'affichent un champ que lorsqu'il sert (les champs du
renfort seulement pour un tenon à renfort, par exemple), et SolidCraft 0.45 ou plus ancien ne sait
pas dessiner un tel formulaire.

1. **Télécharger le paquet.** Ouvrez la
   [dernière version publiée](https://github.com/WilliamHJKL/solidcraft-wood-joinery/releases/latest)
   et, sous *Assets*, téléchargez un seul fichier :
   - `wood-joinery-fr.zip` pour les textes en français ;
   - `wood-joinery-en.zip` pour les textes en anglais.

   Ne prenez pas les deux archives « Source code » : SolidCraft ne peut pas les installer. Ne
   décompressez pas le fichier.
2. **L'installer.** Dans SolidCraft, ouvrez *Preferences › Plugins*, cliquez sur
   *Install plugin…* et choisissez le fichier téléchargé.
3. **Confirmer.** SolidCraft affiche le nom du plugin, sa version et ce qu'il demande. Le plugin
   demande à lancer treize commandes de SolidCraft : pour lire le fichier (`document.inspect`,
   `inspect.measure`, `inspect.interference`, `selection.get`), pour faire les assemblages
   (`solid.box`, `solid.cylinder`, `solid.cone`, `solid.move`, `solid.combine`), pour ranger
   chaque assemblage dans son dossier d'historique (`timeline.group`, `timeline.delete`) et
   garder ses réglages dans le fichier (`storage.get`, `storage.set`, avec la capacité
   `storage`). La capacité `forms` permet à *Modifier un assemblage* d'ouvrir la fenêtre d'un
   assemblage remplie avec ses réglages. Il ne demande aucun accès aux fichiers, à l'horloge ou
   au réseau. Acceptez. Une nouvelle version du plugin redemande cet accord.
4. **Trouver les commandes.** Elles sont dans le groupe *Plugins* de l'onglet *Model*. À partir
   de SolidCraft 0.44, chaque commande y a sa propre icône. SolidCraft 0.42 et 0.43 affichent une
   icône générique, et listent aussi les commandes dans le menu *Plugins*, à côté de *File*.
   Ouvrez un fichier, sélectionnez deux pièces, puis choisissez une commande.

N'installez qu'une seule langue : les deux paquets sont le même plugin, avec le même identifiant.

**Mettre à jour.** Dans *Preferences › Plugins*, cliquez sur *Remove* sur la carte
*Assemblages bois*, puis installez le nouveau fichier comme ci-dessus. Vos fichiers gardent
leurs assemblages : ce sont des fonctions ordinaires.

**Désinstaller.** Dans *Preferences › Plugins*, cliquez sur *Remove* sur la carte du plugin.
L'interrupteur de la même carte désactive le plugin sans le retirer.

**En cas de problème.**

- *SolidCraft refuse le fichier* : vous avez pris une archive « Source code », ou décompressé le
  paquet. Téléchargez de nouveau `wood-joinery-fr.zip` ou `wood-joinery-en.zip` sous *Assets*.
- *Le menu Plugins n'affiche aucune commande* : vérifiez que l'interrupteur du plugin est allumé
  dans *Preferences › Plugins*.
- *Un formulaire affiche « This entry needs to be run from the agent or CLI »* : le plugin est
  plus ancien que la 0.4.3. Retirez-le et installez la dernière version.

Vous pouvez aussi construire les paquets vous-même (voir [Construire](#construire)) ; ils sont
alors dans `dist/`.

## Choisir les pièces

Chaque commande reçoit ses pièces de deux façons :

- **Par clic.** Sélectionnez les pièces dans la vue ou dans le navigateur **avant** d'ouvrir la
  commande, puis laissez vides les champs de nom et lancez-la. Un clic fait pendant que le
  formulaire est ouvert ne remplit pas les champs : SolidCraft dessine le formulaire, et un
  plugin ne peut pas le modifier. Pour un assemblage, la pièce sélectionnée en premier est la
  première pièce (la traverse pour un tenon et mortaise). Une face ou une arête sélectionnée
  compte pour sa pièce.
- **Par nom.** Saisissez les noms tels que le navigateur les affiche.

Pour une rainure, sélectionnez une face plane et laissez vides Pièce et Face : la rainure se
fait dans cette face.

## Aligner d'abord les pièces

Chaque assemblage a une case **Aligner d'abord les pièces** (`align` dans un script). Cochée,
elle fait déplacer la première pièce à sa place avant l'usinage. La deuxième pièce ne bouge
jamais. Le déplacement et l'assemblage font une seule étape d'annulation.

| Assemblage | Où va la première pièce |
|---|---|
| Tenon et mortaise, Tourillons, Faux tenon, Entaille d'étagère, Rainure et languette, Vis, Lamelles | Contre la deuxième pièce : dans sa longueur s'il y a un écart dans ce sens, sinon en travers de l'écart. Sur les côtés, elle ne bouge que du nécessaire pour porter sur la deuxième pièce. Une pièce déjà en place ne bouge pas |
| Queues droites, Queues d'aronde, Coupe d'onglet | Son bout sur la deuxième pièce, au bout le plus proche de celle-ci, centrée sur la largeur. Chaque pièce est lue par son épaisseur, son plus petit côté : un panneau peut être plus haut que long. Deux pièces à plat forment un L : le bout de la première contre le chant de la seconde, à fleur de son bout le plus proche |
| Mi-bois | Dans le plan de la deuxième pièce. Les pièces doivent déjà se croiser, vues de dessus |
| Enture | Bout à bout contre la deuxième pièce, de niveau avec elle ; puis elle glisse dessus de la longueur de la pente |

Case décochée, les pièces doivent déjà être en place, et le plugin le dit quand elles ne le
sont pas, sauf dans trois cas où le plugin les rapproche lui-même quand elles sont à 100 mm au plus : le mi-bois
de pièces chant contre chant ou bout à bout, l'enture de pièces bout à bout, et les vis entre
pièces dont les faces regardent dans le même sens (posées l'une sur l'autre). Plus loin, cochez
la case.

## Tenon et mortaise

![Un tenon à renfort dans un pied, le pied affiché en transparence](docs/mortise-tenon.png)

Sélectionnez, ou saisissez le nom de la traverse et celui du montant, tels que le navigateur les affiche. Le
plugin ajoute le tenon au bout de la traverse et creuse la mortaise correspondante dans le
montant. La boîte englobante de la traverse comprend alors ses tenons : une fiche de débit lit la
vraie longueur de la pièce brute.

Le bout de la traverse doit toucher une face du montant avant de lancer la commande, sauf si
vous cochez **Aligner d'abord les pièces**.

La liste du formulaire affiche le nom français suivi de la valeur d'un script entre parenthèses,
par exemple « à renfort (haunched) ».

| Assemblage | Ce qu'il réalise |
|---|---|
| `classic` : classique (par défaut) | Une mortaise borgne. Le tenon a un épaulement sur chaque chant. |
| `haunched` : à renfort | Une mortaise borgne avec un renfort court, sur le chant de la traverse le plus proche du bout du montant. |
| `through` : traversant | Un tenon qui traverse tout le montant. |
| `bridle` : enfourchement | Une fente ouverte au bout du montant. La languette fait toute la largeur de la traverse. |

Le formulaire affiche chaque règle dans le titre du champ, et les valeurs fixes dans les
champs : 2 mm de réserve de colle, pas de jeu, pas de décalage. Il n'affiche un champ que
lorsqu'il sert : le renfort pour `haunched`, les coins pour `through`, le diamètre des chevilles
et la cheville à tire dès qu'il y a des chevilles. Un script qui donne un champ masqué le voit
ignoré. SolidCraft vérifie aussi les valeurs d'un script d'après le formulaire : chaque pièce
est un seul nom, et un choix s'écrit comme le paquet l'affiche.

Toutes les cotes sont facultatives, en millimètres. Un champ vide suit les usages de la
menuiserie :

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Épaisseur du tenon | `thickness` | Un tiers de l'épaisseur de la traverse |
| Longueur du tenon | `length` | Trois quarts du montant, arrondis à 5 mm ; la mortaise a 2 mm de plus (dégagement de colle). Tout le montant pour Traversant et Enfourchement |
| Épaulement | `shoulder` | La largeur de la joue, au plus un cinquième de la largeur de la traverse. Aucun pour Enfourchement |
| Largeur du renfort | `haunch_width` | L'épaisseur du tenon, au plus un tiers de la largeur de la traverse |
| Profondeur du renfort | `haunch_depth` | L'épaisseur du tenon, au plus 0,4 fois sa longueur |
| Réserve de colle | `glue_relief` | 2 mm sous un tenon borgne |
| Jeu | `clearance` | Aucun |
| Décalage | `offset` | Le tenon est centré dans l'épaisseur de la traverse |
| Tenon double | `double` | Décoché. Coché : deux tenons côte à côte dans l'épaisseur de la traverse, d'un cinquième de cette épaisseur, chacun dans sa mortaise |
| Position du tenon | `position` | Centré. Les deux autres choix mettent le tenon à fleur d'une face de la traverse. Le montant doit alors dépasser la traverse de ce côté |
| Chevilles | `pegs` | 0. Sinon, 1 cheville à mi-longueur du tenon, ou 2 en diagonale, au tiers et aux deux tiers, pour ne pas fendre le montant sur une même ligne. Chaque cheville est un nouveau corps nommé `Peg 1`, `Peg 2`… |
| Diamètre de la cheville | `peg_diameter` | 6 mm, 8 mm si le tenon fait 30 mm de long ou plus |
| Cheville à tire | `drawbore` | 0 : cheville simple. Sinon, le trou du tenon est décalé d'autant vers l'épaulement par rapport à celui du montant (1 à 2 mm) : la cheville serre l'assemblage en entrant |
| Coins | `wedges` | 0. Sinon, 1 ou 2 coins dans le bout d'un tenon traversant, sur les trois quarts de sa longueur. Chaque coin est un nouveau corps nommé `Wedge 1`, `Wedge 2`… |
| Épaisseur des coins | `wedge_thickness` | 4 mm |

Un tenon fait au plus 6 fois son épaisseur en largeur (un tenon double compte pour un). Une
traverse plus large reçoit des tenons jumelés, chacun d'un tiers de la largeur restante, avec entre
eux un renfort aussi profond qu'un renfort de bout : les mortaises restent courtes et la traverse
peut travailler. Toutes les variantes sauf `bridle` le font, et chevilles et coins vont dans chaque
tenon. La réponse indique alors `twin_tenons: 2`.

La réponse signale, sans refuser, trois choses que l'usage évite : une joue de mortaise de moins
de 8 mm, une mortaise en travers du fil du montant (traverse posée à plat), et des tenons jumelés
encore plus larges que 6 fois leur épaisseur. Le journal des
plugins de SolidCraft les affiche aussi.

Exemple : une traverse de 18 × 45 mm contre un montant de 35 × 35 mm reçoit un tenon de
6 × 33 × 25 mm, dans une mortaise de 27 mm de profondeur.

![Un tenon double tenu par deux chevilles, le pied dessiné en transparence](docs/pegged-tenon.png)

![Un tenon traversant avec deux coins dans son bout, le pied dessiné en transparence](docs/wedged-tenon.png)

Dans le modèle, un coin n'écarte pas le bois : il remplit une fente à sa propre forme dans le
bout du tenon, si bien que les volumes restent justes. La mortaise n'est pas évasée.

Le plugin refuse un assemblage impossible et dit pourquoi : un tenon borgne qui traverserait le
montant, un tenon aussi épais que la traverse, une traverse qui ne touche pas le montant, un
enfourchement loin du bout du montant.

## Rainure

![Un pied avec deux rainures arrêtées](docs/groove.png)

Sélectionnez, ou saisissez le nom de la pièce et choisissez une face. La rainure suit le grand côté de cette
face, d'un bout à l'autre, sauf si vous choisissez une autre **Direction**.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Face | `face` | La face sélectionnée. Sinon `+x`, `-x`, `+y`, `-y`, `+z` ou `-z` |
| Direction | `along` | La rainure suit le grand côté de la face. Sinon, l'axe qu'elle suit : `x`, `y` ou `z` |
| Largeur | `width` | 6 mm |
| Profondeur | `depth` | 8 mm |
| Décalage | `offset` | Centrée. Sinon, la distance entre le bord de la face le plus proche de l'origine et la rainure. Zéro donne une feuillure |
| Arrêt au début | `stop_start` | La rainure débouche. Sinon, cette longueur de bois reste au bout le plus proche de l'origine |
| Arrêt à la fin | `stop_end` | Même chose, à l'autre bout |

Sur une face plus large que longue, la même commande réalise une entaille en travers.

## Mi-bois

![Deux pièces croisées à mi-bois, l'une dessinée en transparence](docs/half-lap.png)

Sélectionnez, ou saisissez les noms de deux pièces qui se croisent. Chacune perd la moitié de son épaisseur à
l'endroit du croisement : elles se retrouvent dans le même plan. La même commande réalise un
mi-bois en croix, en T ou d'angle. Des pièces posées sur chant qui se croisent, comme les
croisillons d'un casier, perdent chacune la moitié de leur hauteur. Deux pièces dans le
prolongement l'une de l'autre donnent un mi-bois en bout.

Des pièces qui se croisent doivent se chevaucher dans le modèle : dessinez chaque pièce à sa
taille complète, l'une à travers l'autre. **Aligner d'abord les pièces** ramène dans le même
plan une pièce posée au-dessus ou au-dessous de l'autre.

Deux pièces dans le même alignement n'ont pas besoin de se chevaucher : dessinées chant contre
chant (feuillure à mi-bois) ou bout à bout (mi-bois en bout), jointives ou un peu écartées, la
première glisse sur la seconde du **recouvrement**, puis les deux sont entaillées dans leur
épaisseur. Le panneau perd d'autant en largeur, ou les deux pièces en longueur.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Profondeur dans la première pièce | `depth` | La moitié de l'épaisseur. La deuxième pièce perd le reste |
| Inverser les côtés | `flip` | Décoché : la première pièce est entaillée du côté opposé à l'origine |
| Recouvrement | `overlap` | Pièces chant contre chant : la moitié de leur épaisseur. Bout à bout : la largeur de la première pièce |

## Tourillons

![Deux tourillons entre une traverse et un pied, dessinés en transparence](docs/dowels.png)

Sélectionnez, ou saisissez les noms de deux pièces qui se touchent. Le plugin perce des trous en vis-à-vis dans
les deux, alignés sur le grand côté du contact, et ajoute chaque tourillon comme un nouveau
corps nommé `Dowel 1`, `Dowel 2`, etc.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Diamètre | `diameter` | 6, 8, 10 ou 12 mm : le plus grand qui ne dépasse pas la moitié du petit côté du contact |
| Longueur du tourillon | `length` | Cinq diamètres. Une moitié dans chaque pièce. Une pièce mince garde 3 mm sous le trou, et l'autre pièce reçoit le reste |
| Nombre de tourillons | `count` | Deux s'il y a la place, puis un tous les 96 mm environ |
| Réserve de colle | `glue_relief` | 1 mm au fond de chaque trou |
| Ajouter les tourillons | `dowel_bodies` | Coché. Décoché, seuls les trous sont percés |

Les pièces de moins de 11 mm d'épaisseur sont refusées : il resterait moins de 3 à 5 mm de bois à côté du tourillon.

Exemple : une traverse de 18 × 45 mm contre un montant de 35 × 35 mm reçoit deux tourillons de
8 × 40 mm, 20 mm dans chaque pièce, dans des trous de 21 mm de profondeur.

## Queues droites

![Des queues droites à un angle, la façade dessinée en transparence](docs/box-joint.png)

Sélectionnez, ou saisissez le nom de la pièce dont le bout touche l'autre pièce, puis le nom de cette autre
pièce. La première reçoit des queues carrées qui traversent la deuxième, et la deuxième reçoit
les entailles correspondantes. Comme pour un tenon, la première pièce mesure alors sa vraie
longueur.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Nombre de divisions | `count` | Queues et vides sur la largeur : le nombre impair qui donne les queues les plus carrées, 3 au minimum |
| Commencer par une queue | `swap` | Décoché : la première pièce commence par un vide, du côté le plus proche de l'origine |

Exemple : deux pièces de 18 × 100 mm reçoivent cinq divisions de 20 mm, avec deux queues sur la
première pièce.

## Faux tenon

![Un faux tenon entre une traverse et un pied, dessinés en transparence](docs/loose-tenon.png)

Sélectionnez, ou saisissez les noms de deux pièces qui se touchent. Le plugin creuse deux
mortaises en vis-à-vis et ajoute chaque faux tenon comme un nouveau corps nommé
`Loose tenon 1`, `Loose tenon 2`, etc.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Domino Festool | `domino` | `custom` (sur mesure) : un tenon à bouts carrés, aux cotes ci-dessous. Une taille comme `8x40` (épaisseur × longueur) fait des Dominos Festool : voir plus bas |
| Largeur de la 2e mortaise | `domino_fit` | `medium` (moyenne). Seulement pour un Domino : voir plus bas |
| Épaisseur du tenon | `thickness` | 5, 6, 8 ou 10 mm : la plus grande qui ne dépasse pas le tiers du petit côté du contact |
| Largeur du tenon | `width` | La largeur de l'assemblage, moins un épaulement de chaque côté |
| Longueur du tenon | `length` | Cinq épaisseurs, arrondies aux 10 mm supérieurs. Une moitié dans chaque pièce. Une pièce mince garde 3 mm sous la mortaise, et l'autre pièce reçoit le reste |
| Nombre de tenons | `count` | Un, puis un tous les 120 mm environ |
| Réserve de colle | `glue_relief` | 1 mm au fond de chaque mortaise |
| Ajouter les tenons | `tenon_bodies` | Coché. Décoché, seules les mortaises sont creusées |

Exemple : une traverse de 18 × 45 mm contre un montant de 35 × 35 mm reçoit un faux tenon de
6 × 33 × 30 mm, 15 mm dans chaque pièce, dans des mortaises de 16 mm de profondeur.

### Dominos Festool

Choisissez une taille dans **Domino Festool** : les tenons sont alors des Dominos, aux cotes
de Festool, et les mortaises comme les tenons ont des bouts en demi-cercle, comme les taille la
machine. Les champs épaisseur, largeur et longueur sont alors masqués ; le nombre de tenons, la
réserve de colle et les corps fonctionnent comme plus haut, et les corps s'appellent
`Domino 1`, `Domino 2`, etc.

| Domino | Largeur | Machine |
|---|---|---|
| `4x20`, `5x30`, `6x40` | 16,6, 18,8, 19,8 mm | DF 500, 28 mm de profondeur au plus |
| `8x40`, `8x50`, `10x50` | 21,9, 21,9, 23,9 mm | DF 500 |
| `8x80`, `8x100`, `10x80`, `10x100` | 21,9, 21,9, 23,9, 23,9 mm | DF 700, 70 mm de profondeur au plus |
| `12x100`, `14x100`, `14x140` | 25,9, 27,9, 27,9 mm | DF 700 |

La première mortaise est serrée, de la largeur du Domino. **Largeur de la 2e mortaise** règle
l'autre, comme la molette de la machine : serrée ; moyenne, 6 mm de plus sur la DF 500 et 3 mm
sur la DF 700 ; large, 10 mm de plus sur la DF 500 et 3 mm sur la DF 700 (qui n'a que deux
réglages). Ce jeu permet d'aligner les pièces au collage.

La longueur se partage comme pour tout faux tenon : moitié dans chaque pièce, moins dans une
pièce mince. Un Domino qui demande une mortaise plus profonde que sa machine ne la taille est
refusé : prenez-en un plus court. Le tenon est dessiné à sa longueur nominale ; un vrai Domino
fait environ 2 mm de moins, ce que représente la réserve de colle.

## Entaille d'étagère

![Une étagère logée dans un côté, dans une entaille arrêtée avant le chant avant](docs/housing.png)

Sélectionnez, ou saisissez le nom de l'étagère, puis celui du côté. Le bout de l'étagère doit
toucher une face du côté. Le plugin creuse une entaille de la largeur de l'épaisseur de
l'étagère, et allonge l'étagère de la profondeur de l'entaille : elle mesure ainsi sa vraie
longueur.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Profondeur | `depth` | Un tiers du côté, arrondi au millimètre |
| Arrêt au début | `stop_start` | L'entaille débouche. Sinon, cette longueur de bois reste au bout le plus proche de l'origine |
| Arrêt à la fin | `stop_end` | Même chose, à l'autre bout |
| Pente en queue d'aronde 1:N | `slope` | 0 : une entaille droite. Sinon, une queue d'aronde glissée : la languette s'élargit dans le côté selon cette pente, 6 pour 1:6 |

Exemple : une étagère de 18 mm entre de 6 mm dans un côté de 18 mm.

![Une étagère à languette en queue d'aronde, à moitié sortie de son entaille](docs/sliding-dovetail.png)

Une queue d'aronde glissée retient le côté contre l'étagère. À 1:6 et 6 mm de profondeur, la
languette fait 18 mm de large au fond et 16 mm au collet.

## Coupe d'onglet

![Deux pièces à plat coupées à 45 degrés, dessinées légèrement écartées](docs/miter.png)

Sélectionnez, ou saisissez les noms de deux pièces qui se rejoignent à un angle. Chacune est
coupée selon le plan qui va du coin extérieur au coin intérieur : à 45 degrés si les deux pièces
ont la même épaisseur, à un autre angle sinon.

Les pièces peuvent être dessinées l'une à travers l'autre à l'angle, ou l'une peut s'arrêter
contre l'autre : celle-là est d'abord prolongée jusqu'au coin extérieur. La commande vaut pour
des pièces sur chant, comme les côtés d'une boîte, et pour des pièces à plat, comme les côtés
d'un cadre.

Chaque coupe ajoute trois fonctions dans la timeline : une boîte outil, le déplacement qui la
fait pivoter, et la coupe. La profondeur de languette et les faux tenons n'apparaissent que s'il
y a une languette.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Épaisseur de la languette | `spline` | 0 : pas de languette. Sinon, une languette rapportée de cette épaisseur en travers de la coupe, à mi-longueur. C'est un nouveau corps nommé `Miter spline 1`… |
| Profondeur dans chaque pièce | `spline_depth` | Un tiers de la pièce la plus mince |
| Faux tenons à la place | `keys` | 0 : une languette sur toute la coupe. Sinon, autant de faux tenons (8 au plus), chacun de la moitié de sa part de coupe, nommés `Miter key 1`… |

![Une coupe d'onglet avec une languette rapportée, les pièces dessinées en transparence](docs/miter-spline.png)

## Rainure et languette

![Deux lames d'un panneau, l'une avec une languette et l'autre avec une rainure, dessinées écartées](docs/tongue-groove.png)

Sélectionnez, ou saisissez les noms de deux pièces qui se touchent chant contre chant. La
première reçoit une languette sur tout le chant, la deuxième la rainure correspondante.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Épaisseur de la languette | `thickness` | Un tiers de l'épaisseur des pièces, centrée. Une pièce à languette plus mince que l'autre et affleurant sur une face reçoit une languette bâtarde : la moitié de son épaisseur, contre son autre face, avec un seul arasement |
| Longueur de la languette | `depth` | La moitié de l'épaisseur des pièces, arrondie au millimètre, 10 mm au plus |
| Réserve de colle | `glue_relief` | 1 mm au fond de la rainure |
| Fausse languette | `spline` | Décoché. Coché : les deux pièces sont rainurées, et une lame à part, `Spline 1`, remplit les deux rainures, 2 mm plus courte qu'elles |

Exemple : deux pièces de 18 mm reçoivent une languette de 6 × 9 mm dans une rainure de 10 mm de
profondeur.

## Enture

![Deux pièces aboutées par une longue pente, dessinées légèrement écartées](docs/scarf.png)

Sélectionnez, ou saisissez les noms de deux pièces de même direction qui se chevauchent, bout
sur bout. Chacune est coupée selon une pente qui court sur tout le chevauchement, d'une face à
l'autre. La longueur du chevauchement fixe la pente : huit épaisseurs donnent 1:8.

Des pièces dessinées bout à bout, jointives ou un peu écartées, dans l'alignement, n'ont pas
besoin de se chevaucher : la première glisse sur la seconde de la longueur de la **pente**, 1:8
sauf si vous en donnez une autre, puis les deux sont coupées. L'ensemble perd d'autant en longueur.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Retourner la pente | `flip` | Décoché : la pente part de la face opposée à l'origine |
| À redans, avec une clef | `key` | Décoché : une pente simple. Coché : une enture à redans, voir plus bas |
| Pente 1:N de pièces jointives | `slope` | 8. Des pièces qui se chevauchent déjà gardent leur chevauchement |

Avec **Aligner d'abord les pièces**, la première pièce est déplacée pour que les deux se
chevauchent de huit épaisseurs, alignées en largeur et en épaisseur.

![Une enture à redans : deux pièces en crochet et la clef entre elles, une pièce dessinée en transparence](docs/scarf-key.png)

Une enture à redans a deux marches à la place de la pente. Chaque pièce garde un crochet,
3/8 de l'épaisseur jusqu'au milieu du chevauchement et 5/8 au-delà, et une clef remplit la
marche entre les deux crochets : elle empêche les pièces de se séparer. La clef est un nouveau
corps nommé `Scarf key 1`. Elle fait une demi-épaisseur de long ; un script peut fixer
`key_width`. Le chevauchement doit faire au moins trois épaisseurs. Les bouts sont coupés
d'équerre, sans sifflet.

## Queues d'aronde

![Deux queues d'aronde à un angle, la pièce à tenons dessinée en transparence](docs/dovetail.png)

Sélectionnez, ou saisissez le nom de la pièce à queues, dont le bout touche l'autre pièce, puis
le nom de la pièce à tenons. La première reçoit des queues qui traversent la deuxième, et la
deuxième reçoit les entailles correspondantes, avec un demi-tenon à chaque bord. Donnez un
**Recouvrement** pour des queues semi-cachées.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Nombre de queues | `count` | Une toutes les deux épaisseurs et demie environ |
| Pente 1:N | `slope` | 6, environ 80°. L'usage français prend 5 (environ 78°) ; 8 donne une pente fine pour bois dur |
| Largeur d'un tenon d'aronde | `pin` | La moitié de l'épaisseur de la pièce à queues, au bout étroit du tenon |
| Recouvrement | `lap` | 0 : queues traversantes. Sinon, le bois laissé devant les queues, pour des queues semi-cachées : 4 ou 5 mm sur une devanture de tiroir |

Exemple : deux pièces de 18 × 100 mm reçoivent deux queues, larges de 41 mm au bout et de
35 mm à la base, entre des tenons de 9 mm à leur bout étroit.

Chaque queue ajoute neuf fonctions dans la timeline : la queue est d'abord réalisée comme un
corps à part, ses deux flancs sont coupés à la pente, puis elle creuse son entaille et rejoint
la pièce à queues.

## Vis

Sélectionnez, ou saisissez le nom de la pièce que la vis traverse, puis celui de la pièce où
elle mord. Les deux doivent se toucher. Le plugin perce un trou de passage dans la première
pièce, avec une fraisure pour la tête, et un avant-trou dans la deuxième. Il n'ajoute pas de
corps de vis.

Deux pièces dont les faces regardent dans le même sens, comme deux planches à plat, qui se
chevauchent vues de dessus n'ont pas besoin de se toucher : dessinées dans le même plan ou
écartées, la première est posée sur la seconde, face contre face, avant d'être vissée.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Diamètre de la vis | `diameter` | 4 mm. Le trou de passage fait 0,5 mm de plus, l'avant-trou 0,7 fois la vis |
| Longueur de la vis | `length` | Deux fois et demie la première pièce, arrondie à 5 mm, sans dépasser les deux pièces moins 3 mm |
| Nombre de vis | `count` | Une, deux à partir de 60 mm, puis une tous les 150 mm environ |
| Fraiser les têtes | `countersink` | Coché : une fraisure à 90 degrés, large de deux diamètres |

Exemple : un côté de 18 mm sur un bout d'étagère reçoit deux vis de 4 × 45 mm : trous de 4,5 mm
fraisés à 8 mm, et avant-trous de 3 mm profonds de 27 mm.

## Lamelles

Sélectionnez, ou saisissez les noms de deux pièces qui se touchent. Le plugin creuse dans chaque
pièce la fente que laisse la lame de 100 mm d'une lamelleuse, épaisse de 4 mm, et ajoute chaque
lamelle comme un nouveau corps nommé `Biscuit 1`, `Biscuit 2`, etc.

| Champ | Nom dans un script | Vide signifie |
|---|---|---|
| Taille de la lamelle | `size` | 20. Les tailles 0, 10 et 20 entrent de 8, 10 et 12 mm dans chaque pièce |
| Nombre de lamelles | `count` | Une tous les 150 mm environ |
| Ajouter les lamelles comme corps | `biscuit_bodies` | Coché. Décoché, seules les fentes sont creusées |

Les pièces doivent faire au moins 10 mm d'épaisseur à l'endroit du contact.

## Mettre à jour les assemblages après un changement

Les usinages d'un assemblage sont de simples boîtes et cylindres à cotes fixes : ils ne suivent
pas les paramètres. Le plugin garde donc dans le fichier chaque assemblage qu'il fait, avec ses
réglages et les pièces qu'il relie, et range ses fonctions dans un dossier de l'historique, par
exemple `Mortise and tenon: Traverse / Montant`.

Après un changement de paramètres, cliquez **Mettre à jour les assemblages**. La commande
supprime le dossier de chaque assemblage et refait les assemblages, dans l'ordre où ils ont été
faits, sur les pièces telles qu'elles sont, avec leurs réglages d'origine. Un assemblage fait à
partir de la sélection est refait sur les mêmes pièces, sans rien sélectionner. Le tout s'annule
en une fois.

- Pour retirer un assemblage, supprimez son dossier dans l'historique : la mise à jour l'oublie.
  Ne dégroupez pas le dossier d'un assemblage : la mise à jour s'arrêterait, faute de le trouver.
- Un assemblage qui ne tient plus, par exemple une traverse éloignée de son montant, arrête la
  mise à jour. Le message nomme l'assemblage, et rien n'est modifié.
- Les fonctions d'un assemblage repartent en fin d'historique. Une fonction faite après un
  assemblage, sur la même pièce, passe devant lui après une mise à jour. Une fonction qui se sert
  d'une pièce, comme une Combinaison qui entaille une tablette avec les pieds, voit alors la pièce
  sans ses assemblages.
- SolidCraft permet 256 commandes à un plugin en une exécution, et un fichier d'une trentaine
  d'assemblages en demande plus. La mise à jour refait alors ce qui tient et garde le reste pour
  la passe suivante (un script lit `remaining` dans la réponse). Lancée depuis sa fenêtre, elle
  rouvre cette fenêtre tant qu'il reste des assemblages : recliquez Run jusqu'à ce qu'elle ne
  revienne plus. Chaque passe s'annule à part.
- Quand rien n'a changé depuis la dernière mise à jour menée jusqu'au bout (les mêmes
  assemblages, et chaque corps de même taille et à la même place), la mise à jour ne refait rien
  et laisse le fichier tel quel (un script lit `unchanged` dans la réponse). Cochez **Refaire tous
  les assemblages, même sans changement** pour tout refaire quand même.
- Les assemblages faits avec une version antérieure à 0.12.0 ne sont pas gardés : supprimez-les
  et refaites-les pour pouvoir les mettre à jour.

## Modifier un assemblage

**Modifier** rouvre la fenêtre d'un assemblage, remplie avec les réglages qui l'ont fait.
Sélectionnez d'abord les pièces de l'assemblage (une seule suffit quand elle n'a qu'un
assemblage), ou tapez une partie du nom de son dossier, puis Run. Dans la fenêtre de
l'assemblage, changez ce qu'il faut et cliquez Appliquer : l'assemblage est refait à sa place, et
la mise à jour garde ses nouveaux réglages.

- La fenêtre ne remplace l'assemblage que si elle s'applique aux mêmes pièces. Changez le nom
  d'une pièce et vous faites un nouvel assemblage à côté de l'ancien.
- Une modification prend deux pas d'annulation : l'un pour l'assemblage refait, l'autre pour
  l'ouverture de sa fenêtre.
- Un script ne peut pas ouvrir de fenêtre. Il donne les champs à changer dans `settings`, et
  l'assemblage est refait aussitôt : `{"joint": "Rail / Stile", "settings": {"length": 20}}`.

## Limites de cette version

- Les pièces doivent être alignées sur les axes X, Y ou Z. Le plugin lit la boîte englobante de
  chaque corps : une pièce en biais n'est pas prise en charge. Une face cliquée pour
  une rainure doit aussi être d'équerre avec un axe.
- La profondeur du montant est celle de sa boîte englobante dans l'axe de l'assemblage. Un
  montant avec un pied ou un retour est donc mesuré trop profond : fixez alors vous-même la
  longueur du tenon.
- Les cotes sont des nombres. Elles ne suivent pas seules un paramètre : après un changement,
  cliquez **Mettre à jour les assemblages** (voir plus haut). Un assemblage qui se reconstruit
  seul attend les fonctions paramétriques par plugin, prévues pour une phase ultérieure du RFC
  des plugins de SolidCraft.
- Travaillez avec le composant racine actif. Une pièce placée dans un composant qui a été
  tourné ou déplacé n'est pas encore prise en charge : tournez le corps lui-même, pas son
  composant. Le plugin vérifie où chaque assemblage est arrivé ; s'il n'est pas à l'endroit
  prévu, il ne modifie rien et le dit.
- Un tenon ne s'ajuste pas encore à une rainure déjà creusée dans la traverse ou le montant.
- Un assemblage ne voit pas les autres. Les tourillons de deux traverses posées sur deux faces
  opposées d'un montant mince peuvent se rejoindre au milieu : donnez alors une longueur plus
  courte. Deux pièces déjà reliées par des tenons ou des queues peuvent recevoir des tourillons,
  des vis ou des lamelles : elles sont lues comme se touchant sur la face où la première entre.
- L'enture à redans a des bouts d'équerre et une clef simple, pas une paire de coins croisés. La coupe d'onglet traverse toute la pièce.
- Les coupes en biais ajoutent beaucoup de fonctions : six pour un onglet ou une enture, neuf
  par queue d'aronde.
- SolidCraft arrête une commande de plugin au bout de 30 secondes. Un appel fait donc au plus
  30 tourillons, 30 lamelles, 40 faux tenons, 15 vis, 20 queues d'aronde ou 60 divisions de
  queues droites. Au-delà, la commande refuse et ne modifie rien. Le nombre calculé d'après la
  longueur s'arrête au même chiffre : sur un très long assemblage, les pièces sont plus espacées.
- Les messages d'erreur du plugin sont en anglais dans tous les paquets.

## Lancer depuis un script

Avec `solidcraft-cli`, inspectez le plugin, puis passez la valeur `approval` qu'il renvoie à
`plugin.run`. Les entrées sont `mortise-tenon`, `groove`, `half-lap`,
`dowels`, `box-joint`, `loose-tenon`, `housing`, `miter`, `tongue-groove`, `scarf`, `dovetail`,
`screws` et `biscuits`, plus `update-joints`, sans paramètre, et `edit-joint` (voir Modifier un assemblage).
Tous les assemblages sauf les deux premiers reçoivent les deux corps dans `first` et `second`.
Un script peut omettre les corps et régler d'abord la sélection avec `select.set`.

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

```json
[{"command": "plugin.run", "params": {
  "path": "dist/wood-joinery",
  "approval": "LA_VALEUR_APPROVAL_DE_INSPECT",
  "entry": "dowels",
  "params": {"first": "Rail", "second": "Stile", "diameter": 8, "count": 2}
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

Le script écrit un paquet par langue : `dist/wood-joinery-en.zip` (anglais) et
`dist/wood-joinery-fr.zip` (français).

## Tester

```sh
cargo test                                   # le calcul de chaque usinage
tests/check.py /chemin/vers/solidcraft-cli   # le plugin emballé, dans un vrai SolidCraft
tests/shapes.py /chemin/vers/solidcraft-cli  # chaque assemblage, pièces tournées de 12 façons, reste le même
```

`tests/check.py` lance chaque commande, compare les volumes mesurés à des
valeurs calculées à la main, et vérifie l'annulation, les refus, la réouverture d'un fichier
sans le plugin, et chaque paquet traduit.

## Traduire

SolidCraft affiche les textes d'un plugin tels que son manifeste les donne, dans une seule
langue. Les textes anglais sont dans `plugin/plugin.json`. Une traduction tient en un fichier,
`plugin/locales/<langue>.json`, qui associe chaque texte anglais à sa traduction ;
`plugin/locales/fr.json` sert de modèle.

SolidCraft refuse un manifeste de plus de 64 Kio (16 Kio avant la 0.45). Les textes du
formulaire restent courts quand même : la règle d'un champ est dans son titre, comme
« Épaisseur du tenon (vide = 1/3 de la traverse) », et ce README porte les explications longues.
La construction échoue au-delà de 56 Kio, pour garder de la place aux prochains assemblages.

`./build.sh` écrit alors `dist/wood-joinery-<langue>.zip`. La construction échoue si un texte
n'a pas de traduction, ou si une traduction ne sert plus : une langue ne peut pas prendre de
retard.

## Licence

MIT OR Apache-2.0, comme SolidCraft. `wit/plugin.wit` est l'interface de plugin de SolidCraft
(`craft:plugin@0.1.0`), copiée de son dépôt sous la même licence.
