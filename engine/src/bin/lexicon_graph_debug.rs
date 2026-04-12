use lc_core::db::LocalStorageProvider;
use lc_core::domain::CardMetadata;
use lc_core::storage::{StorageProvider, StoredCard};
use langs::arabic::ArabicMorphology;
use panini_core::Aggregable;
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
    node_type: String, // "root", "lemma", or "pos"
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
    println!("=== Lexicon Graph Debug (Arabic) ===");

    // Graph data for Arabic
    let mut ara_graph = StableGraph::<GraphNode, String>::new();
    let mut ara_node_map: HashMap<String, NodeIndex> = HashMap::new();

    if let Ok(init) = LocalStorageProvider::init(DB_PATH).await {
        let provider = LocalStorageProvider::for_user(init.pool, USER_ID.to_string());
        if let Ok(cards) = provider.fetch_cards().await {
            for card in &cards {
                // Arabic processing
                if let Some(metadata) = extract_metadata::<ArabicMorphology, ()>(card) {
                    if metadata.language != "ara" {
                        continue;
                    }

                    let features = metadata
                        .target_features
                        .iter()
                        .chain(metadata.context_features.iter());
                    for feature in features {
                        let root = feature.morphology.root().unwrap_or_else(|| feature.group_key());
                        let lemma = MorphologyInfo::lemma(&feature.morphology).to_string();
                        let pos = feature.group_key().to_lowercase();

                        // --- Graph Building ---

                        // 1. Root Node
                        let root_id = format!("root_{}", root);
                        let root_node_idx = *ara_node_map.entry(root_id.clone()).or_insert_with(|| {
                            ara_graph.add_node(GraphNode {
                                id: root_id.clone(),
                                label: root.clone(),
                                node_type: "root".to_string(),
                                pos: "root".to_string(),
                                color: "#ffffff".to_string(),
                            })
                        });

                        // 2. PoS Node
                        let pos_node_id = format!("pos_{}", pos);
                        let pos_node_idx = *ara_node_map.entry(pos_node_id.clone()).or_insert_with(|| {
                            let color = match pos.as_str() {
                                "verb" => "#ff4d4d",
                                "noun" => "#2ecc71",
                                "adjective" => "#3498db",
                                "adverb" => "#9b59b6",
                                "pronoun" => "#f1c40f",
                                _ => "#95a5a6",
                            };
                            ara_graph.add_node(GraphNode {
                                id: pos_node_id.clone(),
                                label: pos.to_uppercase(),
                                node_type: "pos".to_string(),
                                pos: pos.clone(),
                                color: color.to_string(),
                            })
                        });

                        // 3. Lemma Node
                        let lemma_id = format!("lemma_{}_{}", lemma, pos);
                        let lemma_node_idx = *ara_node_map.entry(lemma_id.clone()).or_insert_with(|| {
                            let color = match pos.as_str() {
                                "verb" => "#ff4d4d",
                                "noun" => "#2ecc71",
                                "adjective" => "#3498db",
                                "adverb" => "#9b59b6",
                                "pronoun" => "#f1c40f",
                                _ => "#95a5a6",
                            };
                            ara_graph.add_node(GraphNode {
                                id: lemma_id.clone(),
                                label: lemma.clone(),
                                node_type: "lemma".to_string(),
                                pos: pos.clone(),
                                color: color.to_string(),
                            })
                        });

                        // Add Edges
                        // Root -> Lemma
                        if ara_graph.find_edge(root_node_idx, lemma_node_idx).is_none() {
                            ara_graph.add_edge(root_node_idx, lemma_node_idx, "root".to_string());
                        }
                        // PoS -> Lemma
                        if ara_graph.find_edge(pos_node_idx, lemma_node_idx).is_none() {
                            ara_graph.add_edge(pos_node_idx, lemma_node_idx, "pos".to_string());
                        }
                    }
                }
            }
        }
    }

    if ara_graph.node_count() > 0 {
        export_graph_to_html(&ara_graph)?;
    } else {
        println!("No Arabic data found to visualize.");
    }

    Ok(())
}

fn export_graph_to_html(graph: &StableGraph<GraphNode, String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut elements = Vec::new();

    for node_idx in graph.node_indices() {
        let node = &graph[node_idx];
        elements.push(CytoscapeElement {
            data: CytoscapeData::Node(node.clone()),
        });
    }

    for edge_idx in graph.edge_indices() {
        let (source_idx, target_idx) = graph.edge_endpoints(edge_idx).unwrap();
        let edge_type = &graph[edge_idx];
        elements.push(CytoscapeElement {
            data: CytoscapeData::Edge(GraphEdge {
                source: graph[source_idx].id.clone(),
                target: graph[target_idx].id.clone(),
                edge_type: edge_type.clone(),
            }),
        });
    }

    let elements_json = serde_json::to_string_pretty(&elements)?;

    let html_content = format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>Arabic Lexicon Graph</title>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.23.0/cytoscape.min.js"></script>
    <style>
        body {{
            margin: 0;
            padding: 0;
            background-color: #0d0d0d;
            color: #ffffff;
            font-family: 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            overflow: hidden;
        }}
        #cy {{
            width: 100vw;
            height: 100vh;
            display: block;
        }}
        .overlay {{
            position: absolute;
            z-index: 10;
            padding: 24px;
            pointer-events: none;
            width: 100%;
            box-sizing: border-box;
            display: flex;
            justify-content: space-between;
            align-items: flex-start;
        }}
        .controls {{
            pointer-events: auto;
            background: rgba(255, 255, 255, 0.05);
            backdrop-filter: blur(20px);
            padding: 6px;
            border-radius: 14px;
            border: 1px solid rgba(255, 255, 255, 0.1);
            display: flex;
            gap: 4px;
            box-shadow: 0 8px 32px rgba(0,0,0,0.4);
        }}
        button {{
            background: transparent;
            border: none;
            color: rgba(255, 255, 255, 0.6);
            padding: 10px 18px;
            border-radius: 10px;
            cursor: pointer;
            font-size: 13px;
            font-weight: 500;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            letter-spacing: 0.5px;
        }}
        button.active {{
            background: rgba(255, 255, 255, 0.1);
            color: #fff;
            box-shadow: 0 2px 8px rgba(0,0,0,0.2);
        }}
        button:hover:not(.active) {{
            background: rgba(255, 255, 255, 0.05);
            color: rgba(255, 255, 255, 0.9);
        }}
        h1 {{
            margin: 0;
            font-size: 18px;
            font-weight: 300;
            letter-spacing: 4px;
            color: #ffffff;
            opacity: 0.8;
            text-transform: uppercase;
        }}
        .legend {{
            position: absolute;
            bottom: 24px;
            right: 24px;
            background: rgba(255, 255, 255, 0.05);
            backdrop-filter: blur(20px);
            padding: 16px;
            border-radius: 16px;
            border: 1px solid rgba(255, 255, 255, 0.1);
            font-size: 12px;
            box-shadow: 0 8px 32px rgba(0,0,0,0.4);
        }}
        .legend-item {{
            margin: 8px 0;
            display: flex;
            align-items: center;
            color: rgba(255, 255, 255, 0.7);
        }}
        .dot {{
            width: 10px;
            height: 10px;
            border-radius: 3px;
            margin-right: 12px;
        }}
    </style>
</head>
<body>
    <div class="overlay">
        <div class="header">
            <h1>Arabic Lexicon</h1>
        </div>
        <div class="controls">
            <button id="btn-root" class="active" onclick="switchMode('root')">Aggregate by Root</button>
            <button id="btn-pos" onclick="switchMode('pos')">Aggregate by PoS</button>
        </div>
    </div>
    <div id="cy"></div>
    <div class="legend">
        <div class="legend-item"><div class="dot" style="background: #ffffff; box-shadow: 0 0 10px rgba(255,255,255,0.4);"></div> Root Node</div>
        <div class="legend-item"><div class="dot" style="background: rgba(255,255,255,0.2); border: 1px solid #fff;"></div> PoS Category</div>
        <div style="height: 1px; background: rgba(255,255,255,0.1); margin: 12px 0;"></div>
        <div class="legend-item"><div class="dot" style="background: #ff4d4d;"></div> Verb</div>
        <div class="legend-item"><div class="dot" style="background: #2ecc71;"></div> Noun</div>
        <div class="legend-item"><div class="dot" style="background: #3498db;"></div> Adjective</div>
        <div class="legend-item"><div class="dot" style="background: #9b59b6;"></div> Adverb</div>
        <div class="legend-item"><div class="dot" style="background: #f1c40f;"></div> Pronoun</div>
        <div class="legend-item"><div class="dot" style="background: #95a5a6;"></div> Other</div>
    </div>
    <script>
        let currentMode = 'root';
        
        var cy = cytoscape({{
            container: document.getElementById('cy'),
            elements: {}, // elements_json will be injected here
            style: [
                {{
                    selector: 'node',
                    style: {{
                        'label': 'data(label)',
                        'background-color': 'data(color)',
                        'color': '#fff',
                        'text-valign': 'center',
                        'text-halign': 'center',
                        'font-size': '10px',
                        'font-weight': 'bold',
                        'width': 30,
                        'height': 30,
                        'text-outline-width': 1.5,
                        'text-outline-color': '#0d0d0d',
                        'transition-property': 'opacity, background-color, width, height',
                        'transition-duration': '0.3s'
                    }}
                }},
                {{
                    selector: 'node[node_type="root"]',
                    style: {{
                        'width': 50,
                        'height': 50,
                        'font-size': '14px',
                        'background-color': '#fff',
                        'color': '#000',
                        'text-outline-width': 0
                    }}
                }},
                {{
                    selector: 'node[node_type="pos"]',
                    style: {{
                        'width': 60,
                        'height': 60,
                        'font-size': '12px',
                        'border-width': 2,
                        'border-color': '#fff',
                        'background-opacity': 0.8
                    }}
                }},
                {{
                    selector: 'edge',
                    style: {{
                        'width': 1.5,
                        'line-color': 'rgba(255,255,255,0.15)',
                        'curve-style': 'bezier',
                        'opacity': 0.6,
                        'target-arrow-shape': 'triangle',
                        'target-arrow-color': 'rgba(255,255,255,0.15)',
                        'arrow-scale': 0.5
                    }}
                }},
                {{
                    selector: '.hidden',
                    style: {{
                        'display': 'none',
                        'opacity': 0
                    }}
                }}
            ],
            layout: {{
                name: 'cose',
                animate: true,
                animationDuration: 1000,
                componentSpacing: 100,
                nodeRepulsion: 8000
            }}
        }});

        function updateView() {{
            cy.batch(() => {{
                // Handle Edges
                cy.edges().forEach(edge => {{
                    if (edge.data('edge_type') === currentMode) {{
                        edge.removeClass('hidden');
                    }} else {{
                        edge.addClass('hidden');
                    }}
                }});

                // Handle Nodes
                cy.nodes().forEach(node => {{
                    const type = node.data('node_type');
                    if (type === 'lemma') {{
                        node.removeClass('hidden');
                    }} else if (type === currentMode) {{
                        node.removeClass('hidden');
                    }} else {{
                        node.addClass('hidden');
                    }}
                }});
            }});

            const layout = cy.layout({{
                name: 'cose',
                animate: true,
                randomize: false,
                fit: true,
                padding: 100,
                nodeRepulsion: 10000,
                idealEdgeLength: 100
            }});
            layout.run();
        }}

        function switchMode(mode) {{
            if (currentMode === mode) return;
            currentMode = mode;
            
            document.getElementById('btn-root').classList.toggle('active', mode === 'root');
            document.getElementById('btn-pos').classList.toggle('active', mode === 'pos');
            
            updateView();
        }}

        // Initial view setup
        updateView();
    </script>
</body>
</html>
"#, elements_json);

    let output_path = "output/lexicon_graph.html";
    fs::create_dir_all("output")?;
    fs::write(output_path, html_content)?;
    println!("\n>>> Graph visualization exported to: {}", output_path);

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
