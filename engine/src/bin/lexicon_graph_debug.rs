use lc_core::db::LocalStorageProvider;
use lc_core::domain::CardMetadata;
use lc_core::storage::{StorageProvider, StoredCard};
use langs::arabic::ArabicMorphology;
use langs::polish::PolishMorphology;
use panini_core::traits::MorphologyInfo;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;

const DB_PATH: &str = "output/panglot.db";
const USER_ID: &str = "80512005-26ae-4cce-9cdd-48ccc1a3d950";

#[derive(Serialize, Clone)]
struct GraphNode {
    id: String,
    label: String,
    node_type: String, // "root", "lemma", "pos", "case", "aspect"
    pos: String,
    color: String,
}

#[derive(Serialize)]
struct GraphEdge {
    source: String,
    target: String,
    edge_type: String,
}

#[derive(Serialize)]
struct CytoscapeElement {
    data: CytoscapeData,
}

#[derive(Serialize)]
#[serde(untagged)]
enum CytoscapeData {
    Node(GraphNode),
    Edge(GraphEdge),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Lexicon Graph Debug (Multi-Language) ===");

    let mut graphs: HashMap<String, StableGraph<GraphNode, String>> = HashMap::new();
    let mut node_maps: HashMap<String, HashMap<String, NodeIndex>> = HashMap::new();

    if let Ok(init) = LocalStorageProvider::init(DB_PATH).await {
        let provider = LocalStorageProvider::for_user(init.pool, USER_ID.to_string());
        if let Ok(cards) = provider.fetch_cards().await {
            for card in &cards {
                // Process Arabic
                if let Some(metadata) = extract_metadata::<ArabicMorphology, ()>(card) {
                    if metadata.language == "ara" {
                        let graph = graphs.entry("ara".to_string()).or_default();
                        let node_map = node_maps.entry("ara".to_string()).or_default();
                        process_features(graph, node_map, &metadata);
                    }
                }
                // Process Polish
                if let Some(metadata) = extract_metadata::<PolishMorphology, ()>(card) {
                    if metadata.language == "pol" {
                        let graph = graphs.entry("pol".to_string()).or_default();
                        let node_map = node_maps.entry("pol".to_string()).or_default();
                        process_features(graph, node_map, &metadata);
                    }
                }
            }
        }
    }

    if graphs.is_empty() {
        println!("No linguistic data found to visualize.");
    } else {
        for (lang, graph) in graphs {
            export_graph_to_html(&lang, &graph)?;
        }
    }

    Ok(())
}

fn process_features<M>(
    graph: &mut StableGraph<GraphNode, String>,
    node_map: &mut HashMap<String, NodeIndex>,
    metadata: &CardMetadata<M, ()>,
) where 
    M: MorphologyInfo + Serialize + panini_core::Aggregable,
{
    let features = metadata
        .target_features
        .iter()
        .chain(metadata.context_features.iter());

    for feature in features {
        let morphology = &feature.morphology;
        let lemma = morphology.lemma().to_string();
        let pos = morphology.pos_label().to_lowercase();
        
        // --- 1. Lemma Node ---
        let lemma_id = format!("lemma_{}_{}", lemma, pos);
        let lemma_node_idx = *node_map.entry(lemma_id.clone()).or_insert_with(|| {
            graph.add_node(GraphNode {
                id: lemma_id,
                label: lemma.clone(),
                node_type: "lemma".to_string(),
                pos: pos.clone(),
                color: get_pos_color(&pos).to_string(),
            })
        });

        // --- 2. PoS Logic ---
        let pos_id = format!("pos_{}", pos);
        let pos_node_idx = *node_map.entry(pos_id.clone()).or_insert_with(|| {
            graph.add_node(GraphNode {
                id: pos_id,
                label: pos.to_uppercase(),
                node_type: "pos".to_string(),
                pos: pos.clone(),
                color: get_pos_color(&pos).to_string(),
            })
        });
        add_unique_edge(graph, pos_node_idx, lemma_node_idx, "pos");

        // --- 3. Root Logic (if available via fields) ---
        // We look for a field named 'root' via the auto-generated getters
        // (This is a bit hacky but works because MorphologyInfo generates root() for Arabic)
        // Since we are generic, we'll try to get it if we can.
        // Actually, since we know it's Arabic or Polish, we can try to find relevant fields.
        
        // Let's use the field values directly from observations for maximum genericness
        use panini_core::Aggregable;
        for obs_group in morphology.observations() {
            for (field_name, field_value) in obs_group {
                let field_name: String = field_name;
                let field_value: String = field_value;
                match field_name.as_str() {
                    "root" => {
                        let root_id = format!("root_{}", field_value);
                        let root_node_idx = *node_map.entry(root_id.clone()).or_insert_with(|| {
                            graph.add_node(GraphNode {
                                id: root_id,
                                label: field_value.clone(),
                                node_type: "root".to_string(),
                                pos: "root".to_string(),
                                color: "#ffffff".to_string(),
                            })
                        });
                        add_unique_edge(graph, root_node_idx, lemma_node_idx, "root");
                    }
                    "case" => {
                        let case_id = format!("case_{}", field_value);
                        let case_node_idx = *node_map.entry(case_id.clone()).or_insert_with(|| {
                            graph.add_node(GraphNode {
                                id: case_id,
                                label: field_value.clone().to_uppercase(),
                                node_type: "case".to_string(),
                                pos: "case".to_string(),
                                color: "#0984e3".to_string(), // Bright Blue
                            })
                        });
                        add_unique_edge(graph, case_node_idx, lemma_node_idx, "case");
                    }
                    "aspect" => {
                        let aspect_id = format!("aspect_{}", field_value);
                        let aspect_node_idx = *node_map.entry(aspect_id.clone()).or_insert_with(|| {
                            graph.add_node(GraphNode {
                                id: aspect_id,
                                label: field_value.clone().to_uppercase(),
                                node_type: "aspect".to_string(),
                                pos: "aspect".to_string(),
                                color: "#00b894".to_string(), // Mint Green
                            })
                        });
                        add_unique_edge(graph, aspect_node_idx, lemma_node_idx, "aspect");
                    }
                    _ => {}
                }
            }
        }
    }
}

fn add_unique_edge(graph: &mut StableGraph<GraphNode, String>, source: NodeIndex, target: NodeIndex, edge_type: &str) {
    if !graph.edge_indices().any(|idx| {
        let (s, t) = graph.edge_endpoints(idx).unwrap();
        s == source && t == target && graph[idx] == edge_type
    }) {
        graph.add_edge(source, target, edge_type.to_string());
    }
}

fn get_pos_color(pos: &str) -> &'static str {
    match pos {
        "verb" => "#ff4d4d",
        "noun" => "#2ecc71",
        "adjective" => "#3498db",
        "adverb" => "#9b59b6",
        "pronoun" => "#f1c40f",
        "adposition" | "preposition" => "#e67e22",
        "determiner" => "#e74c3c",
        "conjunction" | "coordinatingconjunction" | "subordinatingconjunction" => "#fd79a8",
        _ => "#95a5a6",
    }
}

fn export_graph_to_html(lang: &str, graph: &StableGraph<GraphNode, String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut elements = Vec::new();
    for node_idx in graph.node_indices() {
        elements.push(CytoscapeElement { data: CytoscapeData::Node(graph[node_idx].clone()) });
    }
    for edge_idx in graph.edge_indices() {
        let (source_idx, target_idx) = graph.edge_endpoints(edge_idx).unwrap();
        elements.push(CytoscapeElement {
            data: CytoscapeData::Edge(GraphEdge {
                source: graph[source_idx].id.clone(),
                target: graph[target_idx].id.clone(),
                edge_type: graph[edge_idx].clone(),
            }),
        });
    }

    let elements_json = serde_json::to_string_pretty(&elements)?;
    let title = format!("Panini Lexicon Explorer ({})", lang.to_uppercase());

    let html_content = format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.23.0/cytoscape.min.js"></script>
    <style>
        body {{ margin: 0; padding: 0; background-color: #0d0d0d; color: #ffffff; font-family: 'Inter', sans-serif; overflow: hidden; }}
        #cy {{ width: 100vw; height: 100vh; display: block; }}
        .overlay {{ position: absolute; z-index: 10; padding: 24px; pointer-events: none; width: 100%; display: flex; justify-content: space-between; }}
        .controls {{ pointer-events: auto; background: rgba(255, 255, 255, 0.05); backdrop-filter: blur(20px); padding: 6px; border-radius: 14px; border: 1px solid rgba(255, 255, 255, 0.1); display: flex; gap: 4px; }}
        button {{ background: transparent; border: none; color: rgba(255, 255, 255, 0.6); padding: 10px 18px; border-radius: 10px; cursor: pointer; font-size: 13px; font-weight: 500; transition: all 0.2s; }}
        button.active {{ background: rgba(255, 255, 255, 0.1); color: #fff; }}
        h1 {{ margin: 0; font-size: 18px; font-weight: 300; letter-spacing: 4px; text-transform: uppercase; }}
        .legend {{ position: absolute; bottom: 24px; right: 24px; background: rgba(255, 255, 255, 0.05); backdrop-filter: blur(20px); padding: 16px; border-radius: 16px; border: 1px solid rgba(255, 255, 255, 0.1); font-size: 12px; }}
        .legend-item {{ margin: 8px 0; display: flex; align-items: center; color: rgba(255, 255, 255, 0.7); }}
        .dot {{ width: 10px; height: 10px; border-radius: 3px; margin-right: 12px; }}
    </style>
</head>
<body>
    <div class="overlay">
        <h1>{}</h1>
        <div class="controls">
            <button id="btn-root" class="active" onclick="switchMode('root')">By Root</button>
            <button id="btn-pos" onclick="switchMode('pos')">By PoS</button>
            <button id="btn-case" onclick="switchMode('case')">By Case</button>
            <button id="btn-aspect" onclick="switchMode('aspect')">By Aspect</button>
        </div>
    </div>
    <div id="cy"></div>
    <div class="legend">
        <div class="legend-item"><div class="dot" style="background: #fff;"></div> Pivot Node</div>
        <div style="height: 1px; background: rgba(255,255,255,0.1); margin: 12px 0;"></div>
        <div class="legend-item"><div class="dot" style="background: #ff4d4d;"></div> Verb</div>
        <div class="legend-item"><div class="dot" style="background: #2ecc71;"></div> Noun</div>
        <div class="legend-item"><div class="dot" style="background: #3498db;"></div> Adjective</div>
        <div class="legend-item"><div class="dot" style="background: #0984e3;"></div> Case Node</div>
        <div class="legend-item"><div class="dot" style="background: #00b894;"></div> Aspect Node</div>
    </div>
    <script>
        let currentMode = 'root';
        var cy = cytoscape({{
            container: document.getElementById('cy'),
            elements: {}, 
            style: [
                {{ selector: 'node', style: {{ 'label': 'data(label)', 'background-color': 'data(color)', 'color': '#fff', 'text-valign': 'center', 'font-size': '10px', 'width': 30, 'height': 30, 'text-outline-width': 1.5, 'text-outline-color': '#0d0d0d' }} }},
                {{ selector: 'node[node_type="root"], node[node_type="pos"], node[node_type="case"], node[node_type="aspect"]', style: {{ 'width': 50, 'height': 50, 'font-size': '14px', 'background-color': 'data(color)', 'color': '#fff' }} }},
                {{ selector: 'edge', style: {{ 'width': 1.5, 'line-color': 'rgba(255,255,255,0.15)', 'curve-style': 'bezier', 'opacity': 0.6 }} }},
                {{ selector: '.hidden', style: {{ 'display': 'none', 'opacity': 0 }} }}
            ],
            layout: {{ name: 'cose', animate: true, animationDuration: 1000 }}
        }});

        function updateView() {{
            cy.batch(() => {{
                cy.edges().forEach(e => {{
                    if (e.data('edge_type') === currentMode) e.removeClass('hidden');
                    else e.addClass('hidden');
                }});
                cy.nodes().forEach(n => {{
                    if (n.data('node_type') === 'lemma') {{
                        const edges = n.connectedEdges('[edge_type = "' + currentMode + '"]');
                        if (edges.length > 0) n.removeClass('hidden');
                        else n.addClass('hidden');
                    }} else if (n.data('node_type') === currentMode) {{
                        n.removeClass('hidden');
                    }} else {{
                        n.addClass('hidden');
                    }}
                }});
            }});
            cy.layout({{ name: 'cose', animate: true, fit: true, padding: 50, nodeRepulsion: 10000 }}).run();
        }}

        function switchMode(mode) {{
            currentMode = mode;
            ['root', 'pos', 'case', 'aspect'].forEach(m => {{
                const btn = document.getElementById('btn-'+m);
                if (btn) btn.classList.toggle('active', m === mode);
            }});
            updateView();
        }}
        updateView();
    </script>
</body>
</html>
"#, title, title, elements_json);

    let filename = format!("output/lexicon_graph_{}.html", lang);
    fs::create_dir_all("output")?;
    fs::write(&filename, html_content)?;
    println!("\n>>> Viz exported to: {}", filename);
    Ok(())
}

fn extract_metadata<M, F>(card: &StoredCard) -> Option<CardMetadata<M, F>>
where
    M: for<'de> serde::Deserialize<'de>,
    F: for<'de> serde::Deserialize<'de>,
{
    let fields: Vec<&str> = card.fields.split('\x1f').collect();
    for field in fields.into_iter().rev() {
        if field.trim().starts_with('{') {
            if let Ok(metadata) = serde_json::from_str::<CardMetadata<M, F>>(field) {
                return Some(metadata);
            }
        }
    }
    None
}
