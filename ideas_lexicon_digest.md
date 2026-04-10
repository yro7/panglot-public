# Lexicon Digest — Synthèse des idées

## Point de départ
On a des enums `Morphology` par langue (arabe, japonais, polonais) suivant les Universal Dependencies, avec des subfields très différents par langue. Les features extraites donnent un lexique riche (mot, lemme, racine, cas, genre, temps...). Question : qu'en faire ?

## Idée 1 — Graphe lexical avec nœud central par langue
Le "nœud organisateur" du lexique varie : **racine triconsonantique** en arabe, **lemme** en polonais, **group** pour les verbes japonais. Première tentative : `LexicalAxis` déclaré par langue sur le trait `Language`. Rejeté comme trop plat.

## Idée 2 — Arbre de groupement déclaré par langue
`GroupingNode` avec `BranchBy`/`GroupBy` — chaque langue déclare manuellement sa hiérarchie. Fonctionnel mais verbeux.

## Idée 3 — Dérivation automatique depuis les enums
L'information est **déjà dans les types**. Les champs partagés entre variants (ex: `root` dans Noun/Verb/Adjective en arabe) sont détectables par introspection de la macro. Plus besoin de déclaration manuelle.

## Idée 4 — Pivot libre (modèle OLAP)
L'arbre n'est pas fixe. L'utilisateur choisit l'ordre des axes : voir par lemme, par cas, par racine. C'est un `GROUP BY` récursif, l'ordre est un paramètre runtime. Exemples détaillés pour polonais, arabe, japonais, turc.

## Idée 5 — Le vrai consommateur c'est un LLM
Changement de perspective : le but n'est pas (seulement) la visualisation humaine, c'est nourrir un **agent LLM** qui comprend le profil de l'apprenant pour générer des decks pertinents. Il faut un résumé compact, pas 500 features brutes.

## Idée 6 — La distinction universelle : enum = fermé, String = ouvert
Fondée sur la linguistique (classes ouvertes vs fermées, Bloomfield/Zipf/Shannon) :
- **`enum`** (cas, genre, temps) → ensemble fermé → **distribution** avec couverture (vu 5/7 cas)
- **`String`** (lemme, racine) → ensemble ouvert → **inventaire** (liste des valeurs connues + comptage)

C'est universel, dérivable automatiquement du système de types Rust, et ne nécessite aucune annotation si les champs grammaticaux sont typés en enums (refactoring commencé au commit `50f058e`).

## Idée 7 — Architecture stable : `Aggregator` → `LexiconDigest`

```
ExtractedFeature (brutes, en DB)
        │
        ▼
trait Aggregator  →  implem SchemaAggregator (enum/String)
        │              (interchangeable demain)
        ▼
LexiconDigest  ← structure stable, arbre de Dimensions
        │
        ├──→ to_llm_summary()   → YAML compact pour le prompt
        └──→ to_frontend_tree() → JSON navigable avec pivots
```

`LexiconDigest` contient des `Dimension` qui sont soit `Distribution` (fermé, avec `Coverage { seen, total }`), soit `Inventory` (ouvert, avec liste de valeurs). Structure récursive, chaque `Bin`/`Entry` peut avoir des sous-dimensions.

Le `MorphologySchema` (généré par la macro étendue) fournit les `FieldDescriptor` avec `FieldKind::Closed(variants)` ou `FieldKind::Open`. L'Aggregator est générique, zéro code spécifique par langue.

## Prérequis identifié
Finir de typer les champs grammaticaux en **enums** (au lieu de `String`). Sans ça, `case: String` est indistinguable de `lemma: String` et la dérivation automatique est impossible.
