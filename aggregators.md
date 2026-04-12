# Panini Aggregation System

Ce document décrit l'architecture et l'utilisation du système d'agrégation de la librairie Panini, utilisé pour générer des extraits (digests) de lexiques et des statistiques morphologiques.

## 1. Architecture Core

Le système repose sur une hiérarchie de traits qui permettent de transformer n'importe quelle structure linguistique complexe en données statistiques plates.

### Traits Fondamentaux

*   **`ClosedValues`** : Implémenté par les enums "unitaires" (ex: `Case`, `Person`). Il fournit la liste exhaustive des variantes pour calculer la couverture.
*   **`AggregableFields`** : Définit comment un champ ou un groupe de champs est visualisé (nom, type ouvert ou fermé, valeurs).
*   **`Aggregable`** : Le trait maître. Tout objet implémentant `Aggregable` peut être ingéré par un agrégateur.

### Structure des Données

L'agrégation produit une **`AggregationResult`**, structurée ainsi :
1.  **Groupes** : Le premier niveau (ex: "Noun", "Verb", ou une racine "k-t-b").
2.  **Dimensions** : Les champs agrégés à l'intérieur d'un groupe.
    *   **`Distribution`** (Closed Set) : Pour les données finies (ex: Cas). Permet de savoir que nous avons vu 3 cas sur les 8 possibles.
    *   **`Inventory`** (Open Set) : Pour les données arbitraires (ex: Lemmes, bases).

---

## 2. Le Design Pattern "Pivot"

C'est la fonctionnalité la plus puissante pour l'analyse linguistique. Par défaut, une morphologie se groupe par sa catégorie (PoS). Le pattern **Pivot** permet de changer cet axe d'analyse à la volée.

### Comment ça marche ?
La méthode `.pivoted(|item| key)` enveloppe un objet `Aggregable` dans une structure `Pivoted` qui surcharge la `group_key`.

**Exemple concret (Arabe) :**
Au lieu d'agréger les verbes ensemble, nous pivotons sur la racine :
```rust
// Au lieu de grouper par "Verb", on groupe par la racine "ن ه ر"
let root = feature.morphology.root().unwrap();
agg.record(&feature.pivoted(|_| root.clone()));
```

---

## 3. Outils de Debug & Visualisation

Nous avons deux outils principaux pour inspecter le lexique :

### CLI Debug (`lexicon-debug`)
Affiche les statistiques brutes dans la console. Idéal pour vérifier rapidement la couverture des cas ou le nombre de lemmes uniques.
```bash
cargo run -p engine --bin lexicon-debug
```

### Graph Debug (`lexicon-graph-debug`)
Génère une visualisation interactive HTML (`output/lexicon_graph.html`) utilisant **Cytoscape.js**.
*   **Mode Root** : Visualise les clusters de mots autour de leurs racines trilitères.
*   **Mode PoS** : Visualise les mots regroupés par catégories grammaticales.
*   **Interaction** : Bouton de basculement avec réorganisation animée des nœuds.

```bash
cargo run -p engine --bin lexicon-graph-debug
```

---

## 4. Guide d'Utilisation (Rust)

### Créer un agrégateur simple
```rust
let mut agg = BasicAggregator::new();

for feature in features {
    agg.record(feature);
}

let result = agg.finish();
result.print(); // Affiche les stats formatées
```

### Fusionner des résultats
`AggregationResult` implémente `merge`, ce qui permet d'agréger des données en parallèle puis de les combiner :
```rust
result_a.merge(result_b);
```

### Ajouter une nouvelle catégorie au graphe
Pour qu'un nouveau champ apparaisse dans le graphe interactive, assurez-vous qu'il est extrait dans la boucle principale de `lexicon_graph_debug.rs` et qu'une couleur lui est attribuée dans le match PoS.
```rust
"noun" => "#2ecc71",
"my_new_category" => "#hex_color",
```
