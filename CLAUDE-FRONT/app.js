/* Panglot — skill tree app v3
   - i18next for UI strings
   - PanglotAPI for live mastery data
   - node.name (target language) replaces title:{fr,en,de}
*/
(() => {
  const T = window.PANGLOT_TREE;

  // ===== load custom nodes from localStorage =====
  const CUSTOM_KEY = 'panglot.customNodes.' + T.language.code;
  function loadCustom() {
    try { return JSON.parse(localStorage.getItem(CUSTOM_KEY) || '[]'); } catch { return []; }
  }
  function saveCustom(list) {
    localStorage.setItem(CUSTOM_KEY, JSON.stringify(list));
  }
  function allNodes() { return [...T.nodes, ...loadCustom()]; }

  // ===== i18n =====
  let currentUiLang = localStorage.getItem('panglot.uiLang') || 'fr';
  const uiLangs = [
    { code: 'fr', label: 'Français' },
    { code: 'en', label: 'English' },
    { code: 'de', label: 'Deutsch' }
  ];
  const targetLangs = [
    { code: 'de', native: 'Deutsch', label: { fr: 'Allemand', en: 'German', de: 'Deutsch' }, flag: deFlag() },
    { code: 'tr', native: 'Türkçe', label: { fr: 'Turc', en: 'Turkish', de: 'Türkisch' }, flag: trFlag() },
    { code: 'ru', native: 'Русский', label: { fr: 'Russe', en: 'Russian', de: 'Russisch' }, flag: ruFlag() },
    { code: 'ja', native: '日本語', label: { fr: 'Japonais', en: 'Japanese', de: 'Japanisch' }, flag: jpFlag() }
  ];

  function deFlag() { return `<svg viewBox="0 0 18 13" width="18" height="13"><rect width="18" height="4.33" y="0" fill="#000"/><rect width="18" height="4.33" y="4.33" fill="#D00"/><rect width="18" height="4.34" y="8.66" fill="#FC0"/></svg>`; }
  function trFlag() { return `<svg viewBox="0 0 18 13" width="18" height="13"><rect width="18" height="13" fill="#E30A17"/><circle cx="6" cy="6.5" r="3" fill="#fff"/><circle cx="6.8" cy="6.5" r="2.4" fill="#E30A17"/><polygon points="10,5 10.5,6.5 12,6.5 10.8,7.3 11.2,8.8 10,8 8.8,8.8 9.2,7.3 8,6.5 9.5,6.5" fill="#fff"/></svg>`; }
  function ruFlag() { return `<svg viewBox="0 0 18 13" width="18" height="13"><rect width="18" height="4.33" y="0" fill="#fff"/><rect width="18" height="4.33" y="4.33" fill="#0039A6"/><rect width="18" height="4.34" y="8.66" fill="#D52B1E"/></svg>`; }
  function jpFlag() { return `<svg viewBox="0 0 18 13" width="18" height="13"><rect width="18" height="13" fill="#fff"/><circle cx="9" cy="6.5" r="3.5" fill="#BC002D"/></svg>`; }

  async function initI18n(lang) {
    await i18next.use(i18nextHttpBackend).init({
      lng: lang,
      fallbackLng: 'en',
      backend: { loadPath: 'locales/{{lng}}.json' },
    });
  }

  function t(key, opts) { return i18next.t(key, opts); }

  async function changeLocale(lang) {
    await i18next.changeLanguage(lang);
    currentUiLang = lang;
    localStorage.setItem('panglot.uiLang', lang);
    document.documentElement.lang = lang;
    document.querySelectorAll('[data-i18n]').forEach(el => {
      el.textContent = t(el.getAttribute('data-i18n'));
    });
    document.getElementById('ui-lang-code').textContent = lang.toUpperCase();
    renderSidebar(); renderLevels(); renderNodes(); renderEdges();
    if (activeNodeId) openPanel(activeNodeId, false);
  }

  // ===== layout =====
  const LAYOUT = {
    miniColW: 248,
    rowH: 132,
    padLeft: 180,
    padTop: 150,
    nodeW: 228
  };
  const SUBS_PER_PALIER = 3;
  function palierX(palier) { return LAYOUT.padLeft + palier * SUBS_PER_PALIER * LAYOUT.miniColW; }
  function nodeX(n) { return palierX(n.palier) + n.sub * LAYOUT.miniColW; }

  function computeLayout() {
    const groups = {};
    allNodes().forEach(n => {
      const key = `${n.palier}_${n.sub}`;
      (groups[key] = groups[key] || []).push(n);
    });
    const catOrder = Object.fromEntries(T.categories.map((c, i) => [c.id, i]));
    const positions = {};
    Object.values(groups).forEach(list => {
      list.sort((a, b) => catOrder[a.category] - catOrder[b.category]);
      list.forEach((n, idx) => {
        positions[n.id] = {
          x: nodeX(n),
          y: LAYOUT.padTop + idx * LAYOUT.rowH
        };
      });
    });
    return positions;
  }
  let positions = computeLayout();

  // ===== adequacy zone =====
  function fitZone(n) {
    const lvl = T.nodeLevel(n);
    const d = lvl - T.userFrontier;
    if (n.status === 'mastered') return 'review';
    if (d < -0.3) return 'review';
    if (d <= 0.6) return 'in_zone';
    if (d <= 1.5) return 'ahead';
    return 'beyond';
  }

  // ===== render =====
  const nodesLayer = document.getElementById('nodes-layer');
  const edgesLayer = document.getElementById('edges');
  const satsLayer = document.getElementById('sats-layer');
  const levelsLayer = document.getElementById('levels-layer');

  function statusLabel(s) {
    return t('tree.legend.' + s) || s;
  }

  function renderLevels() {
    levelsLayer.innerHTML = '';
    const totalCols = T.paliers.length * SUBS_PER_PALIER;
    const maxW = LAYOUT.padLeft + totalCols * LAYOUT.miniColW + 200;
    let maxY = LAYOUT.padTop;
    Object.values(positions).forEach(p => { if (p.y > maxY) maxY = p.y; });
    const maxH = maxY + LAYOUT.rowH + 120;
    const canvas = document.getElementById('canvas');
    canvas.style.width = maxW + 'px';
    canvas.style.height = maxH + 'px';

    const palierW = SUBS_PER_PALIER * LAYOUT.miniColW;
    T.paliers.forEach((lv, i) => {
      const div = document.createElement('div');
      div.className = 'level-band';
      const bandX = LAYOUT.padLeft + i * palierW + LAYOUT.nodeW/2 - palierW/2;
      div.style.left = bandX + 'px';
      div.style.width = palierW + 'px';
      const lbl = document.createElement('div');
      lbl.className = 'label';
      const parts = lv.label[currentUiLang].split(' · ');
      lbl.innerHTML = `<b>${parts[0]}</b>${parts[1] || ''}`;
      div.appendChild(lbl);

      for (let s = 0; s < SUBS_PER_PALIER; s++) {
        const tick = document.createElement('div');
        tick.className = 'sub-tick';
        tick.style.left = (s * LAYOUT.miniColW) + 'px';
        tick.innerHTML = `<span>${lv.label.en.split(' · ')[0]}.${s+1}</span>`;
        div.appendChild(tick);
      }

      levelsLayer.appendChild(div);
    });

    const frontier = document.createElement('div');
    frontier.className = 'frontier-line';
    const fx = LAYOUT.padLeft + T.userFrontier * SUBS_PER_PALIER * LAYOUT.miniColW + LAYOUT.nodeW/2;
    frontier.style.left = fx + 'px';
    frontier.innerHTML = `
      <div class="frontier-label">
        <span class="frontier-dot"></span>
        <span>${t('frontier.label')}</span>
      </div>`;
    levelsLayer.appendChild(frontier);
  }

  function renderNodes() {
    nodesLayer.innerHTML = '';
    const reco = allNodes().find(n => fitZone(n) === 'in_zone' && n.status === 'in_progress') ||
                 allNodes().find(n => fitZone(n) === 'in_zone');
    const recoId = reco ? reco.id : null;

    allNodes().forEach(n => {
      const p = positions[n.id];
      const el = document.createElement('div');
      const zone = fitZone(n);
      el.className = `node status-${n.status} zone-${zone}`;
      if (n.custom) el.classList.add('custom');
      el.dataset.id = n.id;
      el.style.left = p.x + 'px';
      el.style.top = p.y + 'px';

      const cat = T.categories.find(c => c.id === n.category);
      const sealIx = T.categories.findIndex(c => c.id === n.category);
      const seals = ['Ph', 'Gr', 'Lx', 'Cj', 'Sy'];
      const sealLetter = cat ? (seals[sealIx] || '?') : '⊕';
      const hue = cat ? cat.hue : 80;
      const isMastered = n.status === 'mastered';
      const sealStyle = isMastered
        ? `background: oklch(96% 0.04 ${hue}); color: oklch(32% 0.12 ${hue}); border-color: transparent;`
        : `background: oklch(94% 0.035 ${hue}); color: oklch(35% 0.12 ${hue}); border-color: oklch(78% 0.06 ${hue});`;

      let practicedHtml = '';
      if (n.lastPracticed != null && n.status !== 'fresh') {
        const d = n.lastPracticed;
        const label = d === 0 ? t('time.today')
                   : d === 1 ? t('time.yesterday')
                   : d < 7   ? t('time.days_ago', { count: d })
                   : d < 30  ? t('time.weeks_ago', { count: Math.round(d/7) })
                              : t('time.over_month');
        practicedHtml = `<div class="n-practiced" title="${n.lastPracticed}j">◷ ${label}</div>`;
      }

      let zoneBadge = '';
      if (zone === 'ahead' || zone === 'beyond') {
        zoneBadge = `<div class="n-zone-badge zone-${zone}" title="${t('zone.' + zone + '.hint')}">${zone === 'beyond' ? '↯' : '↗'} ${t('zone.' + zone + '.label')}</div>`;
      }
      if (zone === 'review' && n.status === 'mastered' && n.lastPracticed > 14) {
        zoneBadge = `<div class="n-zone-badge zone-staleish" title="${t('zone.review.hint')}">◷ ${t('revisit')}</div>`;
      }

      el.innerHTML = `
        ${n.id === recoId ? `<div class="reco">${t('reco')}</div>` : ''}
        ${zoneBadge}
        ${n.custom ? `<div class="n-custom-mark" title="Nœud personnalisé">∿</div>` : ''}
        <div class="n-head">
          <div class="seal" style="${sealStyle}">${sealLetter}</div>
          <div class="n-title">${n.name}</div>
        </div>
        <div class="n-native">${n.native || ''}</div>
        <div class="n-meta">
          <span>${statusLabel(n.status)}</span>
          <span>${Math.round((n.mastery || 0) * 100)}%</span>
        </div>
        <div class="n-bar-track">
          <div class="n-bar-fill" style="width: ${(n.mastery || 0) * 100}%;"></div>
        </div>
        ${practicedHtml}
      `;

      el.addEventListener('click', (e) => { e.stopPropagation(); openPanel(n.id); });
      el.addEventListener('mouseenter', () => highlightChain(n.id));
      el.addEventListener('mouseleave', () => clearHighlight());

      nodesLayer.appendChild(el);
    });
  }

  function renderEdges() {
    const svg = edgesLayer;
    const maxW = parseInt(document.getElementById('canvas').style.width);
    const maxH = parseInt(document.getElementById('canvas').style.height);
    svg.setAttribute('width', maxW);
    svg.setAttribute('height', maxH);
    svg.innerHTML = '';

    allNodes().forEach(n => {
      (n.prereq || []).forEach(preId => {
        const from = positions[preId];
        const to = positions[n.id];
        if (!from || !to) return;
        const x1 = from.x + LAYOUT.nodeW;
        const y1 = from.y + 40;
        const x2 = to.x;
        const y2 = to.y + 40;
        const dx = (x2 - x1);
        const cx1 = x1 + dx * 0.5;
        const cx2 = x2 - dx * 0.5;
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', `M ${x1} ${y1} C ${cx1} ${y1}, ${cx2} ${y2}, ${x2} ${y2}`);
        path.classList.add('edge');
        const zone = fitZone(n);
        if (zone === 'ahead') path.classList.add('edge-ahead');
        if (zone === 'beyond') path.classList.add('edge-beyond');
        path.dataset.from = preId;
        path.dataset.to = n.id;
        svg.appendChild(path);
      });
    });
  }

  function highlightChain(id) {
    const prereqs = new Set(), unlocks = new Set();
    function collectUp(nid) {
      const node = allNodes().find(n => n.id === nid);
      if (!node) return;
      (node.prereq || []).forEach(p => { prereqs.add(p); collectUp(p); });
    }
    function collectDown(nid) {
      allNodes().forEach(n => {
        if ((n.prereq || []).includes(nid)) { unlocks.add(n.id); collectDown(n.id); }
      });
    }
    collectUp(id); collectDown(id);
    const active = new Set([id, ...prereqs, ...unlocks]);

    document.querySelectorAll('.node').forEach(el => {
      el.classList.toggle('dim', !active.has(el.dataset.id));
    });
    document.querySelectorAll('.edge').forEach(p => {
      const from = p.dataset.from, to = p.dataset.to;
      if (active.has(from) && active.has(to)) p.classList.add('active');
      else p.classList.add('dim');
    });
  }
  function clearHighlight() {
    document.querySelectorAll('.node.dim').forEach(el => el.classList.remove('dim'));
    document.querySelectorAll('.edge.active, .edge.dim').forEach(p => { p.classList.remove('active'); p.classList.remove('dim'); });
  }

  // ===== satellites =====
  let activeNodeId = null;
  function renderSatellites(nodeId) {
    satsLayer.innerHTML = '';
    if (!nodeId) return;
    const node = allNodes().find(n => n.id === nodeId);
    if (!node) return;
    const decks = node.decks || [];
    const p = positions[nodeId];
    const cx = p.x + LAYOUT.nodeW / 2;
    const cy = p.y + 60;
    const R = 180;
    decks.forEach((d, i) => {
      const n = decks.length;
      const baseAngle = -Math.PI/2;
      const spread = Math.PI * 0.85;
      const angle = baseAngle - spread/2 + (n === 1 ? spread/2 : (i/(n-1)) * spread);
      const sx = cx + Math.cos(angle) * R;
      const sy = cy + Math.sin(angle) * R;
      const pill = document.createElement('div');
      pill.className = 'deck-sat';
      pill.style.left = sx + 'px';
      pill.style.top = sy + 'px';
      pill.innerHTML = `<span>${d}</span>`;
      satsLayer.appendChild(pill);

      const linkSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      linkSvg.classList.add('sat-link');
      linkSvg.style.position = 'absolute';
      linkSvg.style.left = '0'; linkSvg.style.top = '0';
      linkSvg.setAttribute('width', parseInt(document.getElementById('canvas').style.width));
      linkSvg.setAttribute('height', parseInt(document.getElementById('canvas').style.height));
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path.setAttribute('d', `M ${cx} ${cy} L ${sx} ${sy}`);
      linkSvg.appendChild(path);
      satsLayer.appendChild(linkSvg);

      requestAnimationFrame(() => {
        pill.classList.add('show');
        linkSvg.classList.add('show');
      });
    });
  }

  // ===== pan & zoom =====
  const stage = document.getElementById('stage');
  const canvas = document.getElementById('canvas');
  let vx = 0, vy = 0, vscale = 1;
  function applyTransform() {
    canvas.style.transform = `translate(${vx}px, ${vy}px) scale(${vscale})`;
  }
  function fitView() {
    const W = stage.clientWidth;
    const H = stage.clientHeight;
    const cw = parseInt(canvas.style.width);
    const ch = parseInt(canvas.style.height);
    const s = Math.min(W / cw, H / ch) * 0.92;
    vscale = Math.max(0.4, Math.min(1, s));
    vx = (W - cw * vscale) / 2;
    vy = (H - ch * vscale) / 2;
    applyTransform();
  }
  function zoom(factor, cx, cy) {
    const newScale = Math.max(0.3, Math.min(2.2, vscale * factor));
    const k = newScale / vscale;
    if (cx == null) { cx = stage.clientWidth / 2; cy = stage.clientHeight / 2; }
    vx = cx - (cx - vx) * k;
    vy = cy - (cy - vy) * k;
    vscale = newScale;
    applyTransform();
  }
  document.getElementById('zoom-in').onclick = () => zoom(1.2);
  document.getElementById('zoom-out').onclick = () => zoom(1/1.2);
  document.getElementById('zoom-fit').onclick = fitView;
  const focusBtn = document.getElementById('zoom-focus');
  if (focusBtn) focusBtn.onclick = focusFrontier;

  function focusFrontier() {
    const W = stage.clientWidth, H = stage.clientHeight;
    const fx = LAYOUT.padLeft + T.userFrontier * SUBS_PER_PALIER * LAYOUT.miniColW + LAYOUT.nodeW/2;
    vscale = 1;
    vx = W/2 - fx * vscale;
    const maxY = Math.max(...Object.values(positions).map(p => p.y));
    vy = H/2 - (maxY/2 + LAYOUT.padTop) * vscale;
    applyTransform();
  }

  let dragging = false, dsx = 0, dsy = 0;
  stage.addEventListener('mousedown', (e) => {
    if (e.target.closest('.node') || e.target.closest('.deck-sat') || e.target.closest('.panel') || e.target.closest('.menu') || e.target.closest('.add-node-form')) return;
    dragging = true; dsx = e.clientX - vx; dsy = e.clientY - vy;
    stage.classList.add('dragging');
  });
  window.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    vx = e.clientX - dsx; vy = e.clientY - dsy;
    applyTransform();
  });
  window.addEventListener('mouseup', () => { dragging = false; stage.classList.remove('dragging'); });
  stage.addEventListener('wheel', (e) => {
    e.preventDefault();
    const rect = stage.getBoundingClientRect();
    const cx = e.clientX - rect.left, cy = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.08 : 1/1.08;
    zoom(factor, cx, cy);
  }, { passive: false });

  // ===== panel =====
  const panel = document.getElementById('panel');
  function openPanel(id, animate = true) {
    activeNodeId = id;
    const node = allNodes().find(n => n.id === id);
    if (!node) return;
    const zone = fitZone(node);
    const palierLabel = T.paliers[node.palier].label[currentUiLang];
    const subLabel = `${palierLabel.split(' · ')[0]}.${node.sub + 1}`;
    const catLabel = T.categories.find(c => c.id === node.category)?.label[currentUiLang] || 'Custom';
    document.getElementById('p-cat').textContent = catLabel;
    document.getElementById('p-lvl').textContent = subLabel;
    document.getElementById('p-status').textContent = statusLabel(node.status);
    document.getElementById('p-title').textContent = node.name;
    document.getElementById('p-native').textContent = node.native || '';
    document.getElementById('p-mastery').textContent = Math.round((node.mastery || 0) * 100);
    document.getElementById('p-xdone').textContent = node.exercisesDone || 0;

    const fitBanner = document.getElementById('p-fit');
    fitBanner.className = `fit-banner zone-${zone}`;
    fitBanner.innerHTML = `
      <div class="fit-icon">${zone === 'beyond' ? '↯' : zone === 'ahead' ? '↗' : zone === 'review' ? '◷' : '●'}</div>
      <div class="fit-text">
        <div class="fit-title">${t('zone.' + zone + '.label')}</div>
        <div class="fit-hint">${t('zone.' + zone + '.hint')}</div>
      </div>
    `;

    const dEl = document.getElementById('p-decks');
    dEl.innerHTML = '';
    (node.decks || []).forEach((d, i) => {
      const row = document.createElement('div');
      row.className = 'deck-item';
      const exCount = 6 + ((i * 7) % 14);
      row.innerHTML = `
        <div class="deck-num">${String(i+1).padStart(2,'0')}</div>
        <div class="deck-title">${d}</div>
        <div class="deck-meta">${exCount} ex.</div>
      `;
      dEl.appendChild(row);
    });
    if (!node.decks?.length) dEl.innerHTML = '<span style="color:var(--ink-faded); font-size:12px;">—</span>';

    const prEl = document.getElementById('p-prereq');
    prEl.innerHTML = '';
    (node.prereq || []).forEach(pid => {
      const pn = allNodes().find(n => n.id === pid);
      if (!pn) return;
      const chip = document.createElement('button');
      chip.className = 'req-chip' + (pn.status === 'mastered' ? ' done' : '');
      chip.textContent = pn.name;
      chip.onclick = () => openPanel(pid);
      prEl.appendChild(chip);
    });
    if (!node.prereq?.length) prEl.innerHTML = '<span style="color:var(--ink-faded); font-size:12px;">—</span>';

    const unEl = document.getElementById('p-unlocks');
    unEl.innerHTML = '';
    allNodes().filter(n => (n.prereq || []).includes(id)).forEach(un => {
      const chip = document.createElement('button');
      chip.className = 'req-chip';
      chip.textContent = un.name;
      chip.onclick = () => openPanel(un.id);
      unEl.appendChild(chip);
    });
    if (!unEl.children.length) unEl.innerHTML = '<span style="color:var(--ink-faded); font-size:12px;">—</span>';

    const btn = document.getElementById('btn-start');
    btn.disabled = false;
    if (node.status === 'mastered') btn.textContent = t('node.review');
    else if (node.status === 'in_progress') btn.textContent = t('node.continue');
    else btn.textContent = t('node.start');

    btn.onclick = async () => {
      btn.disabled = true;
      const origLabel = btn.textContent;
      btn.textContent = '…';
      try {
        const result = await PanglotAPI.generateForNode(node.id, T.language.code);
        if (result.success && result.cards?.length) {
          alert(`${result.cards.length} carte(s) générée(s) pour «${node.name}»`);
        } else {
          alert(result.message || 'Erreur lors de la génération');
        }
      } catch (e) {
        alert('Impossible de joindre le serveur');
      } finally {
        btn.disabled = false;
        btn.textContent = origLabel;
      }
    };

    const delBtn = document.getElementById('btn-delete');
    delBtn.style.display = node.custom ? '' : 'none';
    if (node.custom) {
      delBtn.onclick = () => {
        if (!confirm(t('node.delete_confirm'))) return;
        const list = loadCustom().filter(x => x.id !== node.id);
        saveCustom(list);
        positions = computeLayout();
        renderLevels(); renderNodes(); renderEdges();
        panel.classList.remove('open');
        activeNodeId = null;
      };
    }

    panel.classList.add('open');
    document.querySelectorAll('.node').forEach(el => el.classList.toggle('active', el.dataset.id === id));
    renderSatellites(id);
  }
  document.getElementById('panel-close').onclick = () => {
    panel.classList.remove('open');
    document.querySelectorAll('.node').forEach(el => el.classList.remove('active'));
    satsLayer.innerHTML = '';
    activeNodeId = null;
  };
  stage.addEventListener('click', (e) => {
    if (e.target === stage || e.target.classList.contains('canvas')) {
      document.getElementById('panel-close').click();
    }
  });

  // ===== sidebar =====
  function renderSidebar() {
    const all = allNodes();
    const mastered = all.filter(n => n.status === 'mastered').length;
    const total = all.length;
    const overall = Math.round((mastered / total) * 100);
    document.getElementById('overall-pct').textContent = overall;
    document.getElementById('overall-caption').textContent = t('sidebar.nodes_mastered', { mastered, total });

    const lvEl = document.getElementById('level-rows');
    lvEl.innerHTML = '';
    T.paliers.forEach((lv, i) => {
      const levelNodes = all.filter(n => n.palier === i);
      const avg = levelNodes.length ? levelNodes.reduce((s, n) => s + (n.mastery || 0), 0) / levelNodes.length : 0;
      const row = document.createElement('div');
      row.className = 'level-bar';
      row.innerHTML = `
        <span class="lv">${lv.label.en.split(' · ')[0]}</span>
        <span class="track"><span class="fill" style="width:${Math.round(avg*100)}%"></span></span>
        <span class="pct">${Math.round(avg*100)}%</span>
      `;
      lvEl.appendChild(row);
    });

    const catEl = document.getElementById('cat-list');
    catEl.innerHTML = '';
    T.categories.forEach(c => {
      const ns = all.filter(n => n.category === c.id);
      const m = ns.filter(n => n.status === 'mastered').length;
      const row = document.createElement('div');
      row.className = 'cat-row';
      row.innerHTML = `
        <span class="cat-swatch" style="color: oklch(58% 0.12 ${c.hue});"></span>
        <span>${c.label[currentUiLang]}</span>
        <span class="cat-count">${m}/${ns.length}</span>
      `;
      catEl.appendChild(row);
    });

    const curTarget = targetLangs.find(l => l.code === T.language.code) || targetLangs[0];
    document.getElementById('lang-flag').innerHTML = curTarget.flag;
    document.getElementById('lang-native').textContent = curTarget.native;
  }

  // ===== add node form =====
  const addForm = document.getElementById('add-node-form');
  document.getElementById('btn-add-node').onclick = () => {
    addForm.classList.toggle('show');
    const first = addForm.querySelector('input[name=title]');
    if (first) setTimeout(() => first.focus(), 100);
  };
  document.getElementById('add-close').onclick = () => addForm.classList.remove('show');
  function populateAddSelects() {
    const catSel = addForm.querySelector('select[name=category]');
    catSel.innerHTML = T.categories.map(c => `<option value="${c.id}">${c.label[currentUiLang]}</option>`).join('');
    const palSel = addForm.querySelector('select[name=palier]');
    palSel.innerHTML = T.paliers.map((p, i) =>
      [0,1,2].map(s => `<option value="${i}_${s}">${p.label.en.split(' · ')[0]}.${s+1}</option>`).join('')
    ).join('');
    const prBox = addForm.querySelector('.prereq-check');
    prBox.innerHTML = allNodes().map(n => `
      <label><input type="checkbox" value="${n.id}"> ${n.name}</label>
    `).join('');
  }
  document.getElementById('add-submit').onclick = () => {
    const title = addForm.querySelector('[name=title]').value.trim();
    const native = addForm.querySelector('[name=native]').value.trim();
    const cat = addForm.querySelector('[name=category]').value;
    const [palier, sub] = addForm.querySelector('[name=palier]').value.split('_').map(Number);
    const decks = addForm.querySelector('[name=decks]').value.split('\n').map(s => s.trim()).filter(Boolean);
    const prereq = [...addForm.querySelectorAll('.prereq-check input:checked')].map(x => x.value);
    if (!title) { addForm.querySelector('[name=title]').focus(); return; }
    const id = 'custom-' + Date.now().toString(36);
    const newNode = {
      id, category: cat, palier, sub,
      name: title,
      native,
      status: 'fresh',
      mastery: 0,
      exercisesDone: 0,
      lastPracticed: null,
      prereq,
      decks,
      custom: true
    };
    const list = loadCustom();
    list.push(newNode);
    saveCustom(list);
    positions = computeLayout();
    renderLevels(); renderNodes(); renderEdges();
    addForm.classList.remove('show');
    addForm.querySelector('[name=title]').value = '';
    addForm.querySelector('[name=native]').value = '';
    addForm.querySelector('[name=decks]').value = '';
    [...addForm.querySelectorAll('.prereq-check input')].forEach(x => x.checked = false);
    setTimeout(() => openPanel(id), 200);
  };

  // ===== menus =====
  function toggleMenu(menu, anchor) {
    const rect = anchor.getBoundingClientRect();
    menu.style.top = (rect.bottom + 6) + 'px';
    menu.style.left = (rect.right - 200) + 'px';
    menu.classList.toggle('show');
  }
  document.addEventListener('click', (e) => {
    if (!e.target.closest('#lang-btn') && !e.target.closest('#lang-menu')) document.getElementById('lang-menu').classList.remove('show');
    if (!e.target.closest('#ui-lang-btn') && !e.target.closest('#ui-lang-menu')) document.getElementById('ui-lang-menu').classList.remove('show');
  });

  function buildLangMenu() {
    const m = document.getElementById('lang-menu');
    m.innerHTML = '';
    targetLangs.forEach(l => {
      const item = document.createElement('div');
      item.className = 'menu-item' + (l.code === T.language.code ? ' on' : '');
      item.innerHTML = `<span class="flag">${l.flag}</span><span>${l.native}</span><span style="color:var(--ink-faded); font-size:10.5px; margin-left:6px;">${l.label[currentUiLang]}</span>`;
      item.onclick = async () => {
        T.language.code = l.code; T.language.native = l.native;
        await PanglotAPI.fetchAndMerge(l.code);
        positions = computeLayout();
        renderLevels(); renderNodes(); renderEdges();
        buildLangMenu(); renderSidebar();
        m.classList.remove('show');
      };
      m.appendChild(item);
    });
  }
  function buildUiLangMenu() {
    const m = document.getElementById('ui-lang-menu');
    m.innerHTML = '';
    uiLangs.forEach(l => {
      const item = document.createElement('div');
      item.className = 'menu-item' + (l.code === currentUiLang ? ' on' : '');
      item.innerHTML = `<span style="font-family:var(--font-mono); font-size:10px; color:var(--ink-faded); width:22px;">${l.code.toUpperCase()}</span><span>${l.label}</span>`;
      item.onclick = async () => {
        await changeLocale(l.code);
        buildUiLangMenu(); buildLangMenu();
        populateAddSelects();
        m.classList.remove('show');
      };
      m.appendChild(item);
    });
  }
  document.getElementById('lang-btn').onclick = (e) => { e.stopPropagation(); buildLangMenu(); toggleMenu(document.getElementById('lang-menu'), e.currentTarget); };
  document.getElementById('ui-lang-btn').onclick = (e) => { e.stopPropagation(); buildUiLangMenu(); toggleMenu(document.getElementById('ui-lang-menu'), e.currentTarget); };

  // ===== theme =====
  function setTheme(mode) {
    document.documentElement.classList.toggle('dark', mode === 'dark');
    localStorage.setItem('panglot.theme', mode);
    const tw = document.querySelector('[data-tweak="theme"]');
    if (tw) tw.querySelectorAll('button').forEach(b => b.classList.toggle('on', b.dataset.v === mode));
  }
  document.getElementById('theme-btn').onclick = () => {
    const cur = document.documentElement.classList.contains('dark') ? 'dark' : 'light';
    setTheme(cur === 'dark' ? 'light' : 'dark');
  };
  setTheme(localStorage.getItem('panglot.theme') || 'light');

  // ===== tweaks =====
  const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
    "density": "comfy",
    "theme": "light",
    "zones": "on"
  }/*EDITMODE-END*/;
  let tweakState = { ...TWEAK_DEFAULTS };

  function applyTweak(key, val) {
    tweakState[key] = val;
    const container = document.querySelector(`[data-tweak="${key}"]`);
    if (container) container.querySelectorAll('button').forEach(b => b.classList.toggle('on', b.dataset.v === val));
    if (key === 'theme') setTheme(val);
    if (key === 'density') {
      if (val === 'tight') { LAYOUT.miniColW = 210; LAYOUT.rowH = 112; LAYOUT.nodeW = 196; }
      else { LAYOUT.miniColW = 248; LAYOUT.rowH = 132; LAYOUT.nodeW = 228; }
      document.querySelectorAll('.node').forEach(el => el.style.width = LAYOUT.nodeW + 'px');
      positions = computeLayout();
      renderLevels(); renderNodes(); renderEdges();
      fitView();
    }
    if (key === 'zones') {
      document.documentElement.classList.toggle('zones-hidden', val === 'off');
    }
  }
  document.querySelectorAll('.tweak-options button').forEach(b => {
    b.onclick = () => {
      const key = b.closest('.tweak-options').dataset.tweak;
      applyTweak(key, b.dataset.v);
      window.parent.postMessage({ type: '__edit_mode_set_keys', edits: { [key]: b.dataset.v } }, '*');
    };
  });
  window.addEventListener('message', (e) => {
    if (e.data?.type === '__activate_edit_mode') document.getElementById('tweaks').classList.add('show');
    else if (e.data?.type === '__deactivate_edit_mode') document.getElementById('tweaks').classList.remove('show');
  });
  window.parent.postMessage({ type: '__edit_mode_available' }, '*');

  // ===== init =====
  (async () => {
    await initI18n(currentUiLang);
    await PanglotAPI.fetchAndMerge(T.language.code);
    await changeLocale(currentUiLang);
    buildLangMenu(); buildUiLangMenu(); populateAddSelects();
    requestAnimationFrame(() => { fitView(); focusFrontier(); });
  })();
})();
