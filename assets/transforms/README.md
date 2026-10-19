# Deinflection tables

Yomitan's language transforms (`ext/js/language/<lang>/<lang>-transforms.js`),
as JSON files exported by Fushi (`fushi/assets/transforms/`, commit `ac8034e`).
Yomitan and Fushi are GPL-3.0, like Nikomiru.

Schema (SPECIFICATION LKP-020): `conditions` (with `subConditions`),
`transforms` with `rules` of type `suffix`, `prefix` or `wholeWord`, each with
`conditionsIn` and `conditionsOut`. `manifest.json` lists the languages;
`en_irregular.json` holds the irregular English forms.

Do not edit these files by hand: update them from upstream and record the
commit here.
