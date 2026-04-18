const PanglotAPI = (() => {
  const BASE = 'http://localhost:8080';
  let _token = null;

  function headers() {
    const h = { 'Content-Type': 'application/json' };
    if (_token) h['Authorization'] = 'Bearer ' + _token;
    return h;
  }

  function flattenTree(node, out = {}) {
    out[node.id] = node;
    (node.children || []).forEach(c => flattenTree(c, out));
    return out;
  }

  function computeFrontier(nodes) {
    const lvl = window.PANGLOT_TREE.nodeLevel;
    const inProg = nodes.filter(n => n.status === 'in_progress');
    if (inProg.length) return inProg.reduce((s, n) => s + lvl(n), 0) / inProg.length;
    const mastered = nodes.filter(n => n.status === 'mastered');
    if (mastered.length) return Math.max(...mastered.map(lvl)) + 0.5;
    return 0;
  }

  async function fetchAndMerge(lang = 'de') {
    let backendMap = {};
    try {
      const res = await fetch(`${BASE}/api/tree?lang=${lang}`, { headers: headers() });
      if (res.ok) backendMap = flattenTree(await res.json());
    } catch (e) {
      console.warn('[PanglotAPI] GET /api/tree failed, using static defaults', e);
    }
    window.PANGLOT_TREE.nodes.forEach(n => {
      const b = backendMap[n.id];
      if (b) {
        n.mastery       = b.mastery;
        n.status        = b.status;
        n.exercisesDone = b.exercises_done;
        if (b.name) n.name = b.name;
      }
    });
    window.PANGLOT_TREE.userFrontier = computeFrontier(window.PANGLOT_TREE.nodes);
  }

  async function generateForNode(nodeId, lang = 'de') {
    const res = await fetch(`${BASE}/api/generate`, {
      method: 'POST',
      headers: headers(),
      body: JSON.stringify({
        node_id: nodeId,
        language: lang,
        user_profile: {
          ui_language: 'English',
          linguistic_background: [],
          srs_algorithm: 'sm2',
          learn_ahead_minutes: 20,
        },
      }),
    });
    return res.json();
  }

  return { fetchAndMerge, generateForNode, setToken: t => { _token = t; } };
})();
