# Migration Panglot → `extract_with_components`

## Contexte

Panini expose deux API d'extraction :

| API | Retour | Type safety |
|-----|--------|-------------|
| `extract_features_via_llm` (legacy) | `FeatureExtractionResponse<M, F>` typé | Compile-time |
| `extract_with_components` (composable) | `ExtractionResult` (JSON wrappé + `.get::<T>(key)`) | Runtime |
| `#[derive(PaniniResult)]` | Struct custom typé (appelle `extract_with_components` en interne) | Compile-time |

Panglot utilise actuellement la legacy. On veut passer sur la version composable.

## `#[derive(PaniniResult)]` — pourquoi on ne peut pas l'utiliser directement

Le derive émet une bound `ComponentRequires<L>` pour **chaque** champ. `MorphemeSegmentation` n'implémente `ComponentRequires<L>` que si `L: Agglutinative`.

`Pipeline<L>` est générique sur `L: Language` — il doit compiler pour le polonais (non-agglutinant) ET le turc (agglutinant). Donc un struct unique avec `#[component(MorphemeSegmentation)]` ne compile pas pour toutes les langues.

Rendre les champs `Option` ne résout pas le problème : le derive émet la bound `ComponentRequires` même pour les `Option`. C'est un choix de design délibéré — `Option` signifie "le LLM peut ne pas l'avoir retourné", pas "le composant n'est pas compatible".

## Trois options

### Option A — `extract_with_components` direct (pragmatique)

Appeler `extract_with_components` manuellement, construire la liste des 4 composants, unpacker avec `.get::<T>(key)`.

- **+** Simple, un seul fichier modifié (`worker.rs`)
- **+** `MorphemeSegmentation` auto-filtré au runtime par `is_compatible()`
- **−** Clés string (`"morphology"`, etc.) — une typo = erreur runtime
- **Mitigation** : utiliser `component.schema_key()` au lieu de strings hardcodées

### Option B — Type associé sur `Language` (maximaliste)

1. Trait `ExtractionOutput<M, F>` avec accesseurs uniformes
2. Type associé `Language::Extraction: ExtractionOutput<...>`
3. Chaque langue définit son propre struct `#[derive(PaniniResult)]`
4. Le pipeline appelle `L::Extraction::extract(...)` génériquement

- **+** Type-safe à la compilation, même pour `MorphemeSegmentation`
- **−** Nouveau trait, un struct par langue, un impl par langue, un type associé en plus
- **−** Boilerplate significatif pour sécuriser une seule clé optionnelle

### Option C — Patcher le derive dans Panini

Ne pas émettre `ComponentRequires` pour les champs `Option`, permettant un struct unique pour toutes les langues.

- **+** Zéro boilerplate côté Panglot
- **−** Casse la sémantique de `Option` : `None` signifierait "erreur LLM" OU "composant incompatible" — deux causes distinctes, indiscernables

## Recommandation

**Option A** pour avancer maintenant. La type safety "perdue" est minimale : les clés sont des constantes statiques des composants, et la validation JSON + désérialisation serde attrape toute incohérence au runtime. L'option B reste disponible si le nombre de composants language-specific augmente.
