# Panini Aggregation System: Developer Guide

This document provides technical details and concrete examples for developers who want to extend or modify the aggregation logic.

---

## 1. File Map: Where to find things

| Concept                | Location                                | Purpose                                                                                    |
| :--------------------- | :-------------------------------------- | :----------------------------------------------------------------------------------------- |
| **Traits Definition**  | `panini-core/src/aggregable.rs`         | Definitions of `Aggregable`, `Aggregator` and `ClosedValues`.                              |
| **Aggregation Engine** | `panini-core/src/aggregable/digest.rs`  | Implementation of `BasicAggregator`, `AggregationResult`, `Distribution`, and `Inventory`. |
| **Macro Logic**        | `panini-macro/src/morphology_info.rs`   | The code that automatically implements `Aggregable` and generates getters.                 |
| **Debug CLI**          | `engine/src/bin/lexicon_debug.rs`       | The main entry point for console reports.                                                  |
| **Interactive Graph**  | `engine/src/bin/lexicon_graph_debug.rs` | The logic for Cytoscape.js export.                                                         |

---

## 2. Scenario: Adding a new Analysis Field

If you add a field to a linguistic morphology (e.g., adding `Aspect` to a Verb), the system can pick it up automatically.

### Step A: Update the Language Definition
In `panini-langs/src/arabic.rs` (or your language file):
```rust
#[derive(..., ClosedValues)] // 1. Derive ClosedValues for enums
pub enum ArabicAspect { Perfective, Imperfective }

pub enum ArabicMorphology {
    Verb {
        lemma: String,
        root: String,
        aspect: ArabicAspect, // 2. Add the field to the variant
        // ...
    }
}
```

### Step B: Verification
You don't need to touch the aggregator! Because of the macro, the next time you run:
```bash
cargo run --bin lexicon-debug
```
The console will automatically show:
```text
  |- aspect [2/2]: perfective(12), imperfective(8)
```

---

## 3. Scenario: Creating a Custom Pivot

The "Pivot" allows you to re-bucket results. You can combine fields for more complex analysis.

### Multi-field Pivot
Example: Analyze verbs by both Root AND Tense.
```rust
let mut agg = BasicAggregator::new();

for feature in features {
    // Generate a combined key
    let key = format!("{}-{}", 
        feature.morphology.root().unwrap_or_default(),
        feature.morphology.tense().unwrap_or_default()
    );
    
    // Record using the pivot
    agg.record(&feature.pivoted(|_| key.clone()));
}
```

---

## 4. Scenario: Implementing a Custom Aggregator

If you need a different output format (e.g., exporting to JSON for a web frontend), implement the `Aggregator` trait.

```rust
pub struct JsonAggregator {
    data: Vec<serde_json::Value>,
}

impl Aggregator for JsonAggregator {
    type Output = String;

    fn record<A: Aggregable>(&mut self, item: &A) {
        // Collect all observations into a simple JSON map
        let mut map = serde_json::Map::new();
        map.insert("group".into(), item.group_key().into());
        
        for obs in item.observations() {
            for (k, v) in obs {
                map.insert(k, v.into());
            }
        }
        self.data.push(serde_json::Value::Object(map));
    }

    fn finish(self) -> String {
        serde_json::to_string_pretty(&self.data).unwrap()
    }
}
```

---

## 5. Important Implementation Details

> [!IMPORTANT]
> **Open vs. Closed Sets**
> - Fields returning `FieldKind::Closed` (via `ClosedValues`) allow the system to calculate **Coverage** (percentage of presence vs. total possibilities).
> - Fields returning `FieldKind::Open` (like `lemma` or `root`) are stored in an `Inventory` that only tracks counts of observed strings.

> [!WARNING]
> **Performance**
> Avoid calling `item.observations()` multiple times in a loop. The `record` method is designed to be the single entry point. If you need custom logic, wrap it in a `pivoted()` closure or a new `Aggregator` implementation.
