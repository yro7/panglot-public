# Refactor: Language trait — héritage → composition

## Context

`Language: LinguisticDefinition` force chaque langue migrée (Polish, Arabic, Turkish) à implémenter `LinguisticDefinition` dans Panglot en déléguant 5-8 méthodes vers `panini_langs`. C'est du boilerplate pur. La composition (`linguistic_def()`) élimine cette duplication tout en gardant Panini standalone.

## Approche

Remplacer le supertrait `Language: LinguisticDefinition` par un type associé `LinguisticDef` + méthode `linguistic_def()`.

## Étapes

### 1. Modifier le trait `Language` — [core/src/traits.rs](core/src/traits.rs)

```rust
pub trait Language {
    type Morphology: Debug + Clone + Serialize + for<'de> Deserialize<'de>
        + schemars::JsonSchema + MorphologyInfo + Send + Sync;
    type GrammaticalFunction: Debug + Clone + PartialEq
        + Serialize + for<'de> Deserialize<'de>
        + schemars::JsonSchema + Send + Sync;
    type ExtraFields: schemars::JsonSchema + Debug + Clone + Serialize + for<'de> Deserialize<'de>;
    type LinguisticDef: LinguisticDefinition<
        Morphology = Self::Morphology,
        GrammaticalFunction = Self::GrammaticalFunction,
    > + Send + Sync;

    fn linguistic_def(&self) -> &Self::LinguisticDef;

    fn generation_directives(&self) -> Option<&str> { None }
    fn ipa_strategy(&self) -> IpaConfig { IpaConfig::None }
    fn tts_strategy(&self) -> TtsConfig { TtsConfig::None }
    fn default_tree_config(&self) -> SkillTreeConfig {
        resolve_config(self.linguistic_def().iso_code().to_639_3())
    }
}
```

### 2. Langues migrées — supprimer `impl LinguisticDefinition`, simplifier

**[langs/src/polish.rs](langs/src/polish.rs)** — supprimer les lignes 9-32 (`impl LinguisticDefinition`), ajouter types associés :
```rust
impl Language for Polish {
    type Morphology = PolishMorphology;
    type GrammaticalFunction = ();
    type ExtraFields = NoExtraFields;
    type LinguisticDef = panini_langs::polish::Polish;
    fn linguistic_def(&self) -> &panini_langs::polish::Polish { &panini_langs::polish::Polish }
    // generation_directives, ipa_strategy, tts_strategy inchangés
}
```

**[langs/src/arabic.rs](langs/src/arabic.rs)** — idem, `type LinguisticDef = panini_langs::arabic::Arabic`

**[langs/src/tur.rs](langs/src/tur.rs)** — idem, supprime les 8 méthodes déléguées (lignes 9-47), `type LinguisticDef = panini_langs::turkish::Turkish`

Nettoyer les imports : retirer `LinguisticDefinition, Script, TypologicalFeature, IsoLang` des `use` si plus utilisés.

### 3. Langues non-migrées — ajouter les types associés

**[langs/src/rus.rs](langs/src/rus.rs), [langs/src/kor.rs](langs/src/kor.rs), [langs/src/cmn.rs](langs/src/cmn.rs), [langs/src/japanese.rs](langs/src/japanese.rs)** :

Garder `impl LinguisticDefinition for X { ... }` tel quel. Ajouter dans `impl Language` :
```rust
type LinguisticDef = Self;
fn linguistic_def(&self) -> &Self { self }
```

Et déplacer `type Morphology` / `type GrammaticalFunction` depuis `impl LinguisticDefinition` vers `impl Language` (ils doivent être sur les deux — le bound `LinguisticDef: LinguisticDefinition<Morphology = Self::Morphology>` l'exige).

### 4. Call sites engine — ajouter `.linguistic_def()`

**[engine/src/pipeline.rs](engine/src/pipeline.rs)** — 8 sites :
- L171, L215, L405, L457, L758, L874 : `self.language.iso_code()` → `self.language.linguistic_def().iso_code()`
- L754 : `self.language.name()` → `self.language.linguistic_def().name()`
- L862 : `self.language.build_extraction_schema()` → `self.language.linguistic_def().build_extraction_schema()`

**[engine/src/prompts.rs](engine/src/prompts.rs)** — L241 :
- `self.language.name()` → `self.language.linguistic_def().name()`

**[engine/src/post_process.rs](engine/src/post_process.rs)** — L89, L163 :
- `language.name()` → `language.linguistic_def().name()`

**[engine/src/card_models.rs](engine/src/card_models.rs)** — L63 :
- `language.typological_features()` → `language.linguistic_def().typological_features()`

### 5. Appel d'extraction — [engine/src/feature_extractor.rs](engine/src/feature_extractor.rs)

L74-75 : `panini_engine::extract_features_via_llm(language, ...)` → `panini_engine::extract_features_via_llm(language.linguistic_def(), ...)`

Le `where` clause reste identique (bounds sur `L::Morphology`, `L::GrammaticalFunction`).

### 6. Vérifier `langs/build.rs`

Le build script extrait `type Morphology = X` par regex sur tout le fichier. Après refacto, cette ligne apparaît dans `impl Language` (migrées) ou `impl LinguisticDefinition` (non-migrées). Le regex n'est pas sensible au bloc `impl` — **pas de changement nécessaire**, mais vérifier que `cargo build` passe.

## Fichiers modifiés (13)

| Fichier                           | Changement                                     |
| --------------------------------- | ---------------------------------------------- |
| `core/src/traits.rs`              | Refacto du trait Language                      |
| `langs/src/polish.rs`             | Suppr `impl LinguisticDefinition`, ajout types |
| `langs/src/arabic.rs`             | Idem                                           |
| `langs/src/tur.rs`                | Idem (8 méthodes supprimées)                   |
| `langs/src/rus.rs`                | Ajout `type LinguisticDef = Self`              |
| `langs/src/kor.rs`                | Idem                                           |
| `langs/src/cmn.rs`                | Idem                                           |
| `langs/src/japanese.rs`           | Idem                                           |
| `engine/src/pipeline.rs`          | 8x `.linguistic_def()`                         |
| `engine/src/prompts.rs`           | 1x `.linguistic_def()`                         |
| `engine/src/post_process.rs`      | 2x `.linguistic_def()`                         |
| `engine/src/card_models.rs`       | 1x `.linguistic_def()`                         |
| `engine/src/feature_extractor.rs` | 1x passage à panini                            |

## Vérification

```bash
cargo build --workspace        # Compilation — doit passer sans erreur
cargo test --workspace         # Tous les tests existants
cargo clippy --workspace       # Pas de nouveaux warnings
```
