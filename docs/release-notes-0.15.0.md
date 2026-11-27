Woodworking joints for SolidCraft, as a command plugin. In this version you pick the boards with a click, a docked *Joints* panel lists the joints of the design, and the French package speaks French. It is made for SolidCraft 0.51.

## Install

You need the SolidCraft desktop application, **version 0.51.0 or later**. On 0.50 everything works but opening a joint's form from the Joints panel. On SolidCraft 0.46 to 0.49, install [v0.13.0](https://github.com/WilliamHJKL/solidcraft-wood-joinery/releases/tag/v0.13.0) instead.

1. Download `wood-joinery-en.zip` (English) or `wood-joinery-fr.zip` (French) below. Do not take the "Source code" archives, and do not unzip the file.
2. In SolidCraft, open Preferences › Plugins. If an older version is installed, click Remove on its card first.
3. Click Install plugin… and pick the file. The plugin asks for the same commands as 0.14.0, and for one more capability, `ui`, for the Joints panel.

Install one package only. Both have the same plugin id. Your designs keep their joints, and Edit a joint and Update joints read the joints made with older versions.

## New

- **Pick the boards with a click.** The board fields of the thirteen joints work like Extrude's *Profiles*: the first one is active when the form opens, a click on a board in the view or in the browser fills it, and the next field takes the next click. Each field shows the board's name and a cross to empty it. Boards selected before you open the form still fill it.
- **The Joints panel.** Docked on the right, it lists the joints of the open design with their state, and says whether the boards changed since the last update. *Update joints* and *Refresh* are its buttons; a double click on a row opens that joint's form (SolidCraft 0.51). SolidCraft does not yet tell a panel when a design opens (tangency-labs/solidcraft#21824): press *Refresh* to fill the list.
- **Messages in French.** The French package gives its warnings and refusals in French, with a decimal comma: "la mortaise laisse une joue de 6,0 mm dans le montant…". The names of the joints' folders stay in English, as the designs keep them.

## Fixed

- **Edit a joint applies at the first click.** Apply no longer asks to check the form and to be pressed again: opening the form changes nothing in the design. The form shows, greyed at its foot, the joint it replaces.

## Changed, for scripts

A board is now given as a list of one name, `{"rail": ["Rail"], "stile": ["Stile"]}`, or as the reference a click sends, `[{"type": "body", "name": "Rail"}]`. SolidCraft refuses a plain name. Leaving the boards out and selecting them first with `select.set` still works.

Joints, sizes and results are the same as in 0.14.0.

## Tested

On SolidCraft 0.51.1 and 0.50.0: 96 unit tests, 208 end-to-end checks and 13 joints in 12 orientations each, from the command line. In the 0.51.1 app: a mortise and tenon picked with two clicks (one in the browser, one in the view) and made at the first Run, the Joints panel in French with its Refresh button, and a double click on its row opening the joint's form, filled, which replaced the joint at the first click.
