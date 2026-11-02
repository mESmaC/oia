// OIA shell spike: presentation layer only. All state lives in Rust and is
// read through typed commands; this file draws it and sends user intents back.
// No value shown here is invented in the UI: every readout comes from a
// command response or from the actual viewport.
'use strict';

const tauri = window.__TAURI__;
if (!tauri || !tauri.core) {
  document.body.textContent = 'No runtime: this page only works inside the spike window.';
  throw new Error('tauri runtime missing');
}
const invoke = tauri.core.invoke;
const listen = tauri.event.listen;

const S = {
  view: null,        // ShellView from Rust
  home: null,        // CaseHome
  geometry: null,    // CaseGeometry
  marks: null,       // timeline marks
  jobs: [],          // Job[]
  object: null,      // ObjectDetail for the selection
  docs: null, report: null, exportPreview: null,
  search: null,      // applied SearchResult, or null
  pendingSearchJob: null,
  inspectorTab: 'selected',
  startupMs: null,
  dismissedNotices: new Set(),
};

const $ = (id) => document.getElementById(id);
const SVG_NS = 'http://www.w3.org/2000/svg';

function el(tag, attrs, ...children) {
  const node = document.createElement(tag);
  setAttrs(node, attrs);
  append(node, children);
  return node;
}
function svg(tag, attrs, ...children) {
  const node = document.createElementNS(SVG_NS, tag);
  setAttrs(node, attrs);
  append(node, children);
  return node;
}
function setAttrs(node, attrs) {
  if (!attrs) return;
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === 'class') node.setAttribute('class', v);
    else if (k.startsWith('on') && typeof v === 'function') node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v === true ? '' : String(v));
  }
}
function append(node, children) {
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    node.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}
function clear(node) { while (node.firstChild) node.removeChild(node.firstChild); }
function badge(b, extraClass) {
  const tone = b.tone && b.tone !== 'neutral' ? b.tone : '';
  return el('span', { class: `badge ${tone} ${extraClass || ''}`.trim() }, el('span', { class: 'g', 'aria-hidden': 'true' }, b.glyph), b.label);
}
function hhmm(min) { return `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`; }
function errText(e) { return typeof e === 'string' ? e : (e && e.message) || String(e); }

// ── Shell state round trips ───────────────────────────────────────────────
async function applyShell(promise) {
  try {
    const env = await promise;
    S.view = env.data;
    applySettings();
    await loadSelection();
    renderAll();
  } catch (e) {
    notice(errText(e), 'bad');
  }
}
function shell() { return S.view.shell; }
function applySettings() {
  const s = shell().settings;
  const root = document.documentElement;
  root.dataset.reduceMotion = String(s.reduced_motion);
  root.dataset.reduceTransparency = String(s.reduced_transparency);
  root.dataset.effects = s.effects;
  root.dataset.clean = String(s.clean_evidence);
  root.dataset.focused = String(s.focused_reading);
  root.style.setProperty('--reading-px', `${s.reading_text_px}px`);
}
async function loadSelection() {
  const sel = shell().context.selection;
  if (!sel) { S.object = null; return; }
  if (S.object && S.object.id === sel.object_id && S.object.version === sel.version) return;
  try {
    S.object = (await invoke('object_detail', { objectId: sel.object_id })).data;
  } catch (e) {
    S.object = null;
    notice(errText(e), 'bad');
  }
}

// ── Boot ──────────────────────────────────────────────────────────────────
async function boot() {
  const [view, home, geo, marks, jobs] = await Promise.all([
    invoke('shell_state'), invoke('case_home'), invoke('case_geometry'), invoke('timeline_marks'), invoke('list_jobs'),
  ]);
  S.view = view.data; S.home = home.data; S.geometry = geo.data; S.marks = marks.data; S.jobs = jobs.data;
  // The window may still be resizing while the first frame renders; redraw on every size change.
  window.addEventListener('resize', () => { renderCanvas(); renderReadout(); });
  applySettings();
  await loadSelection();
  renderAll();
  reportRestore();
  await listen('jobs-changed', onJobsChanged);
  const ready = await invoke('ui_ready');
  S.startupMs = ready.data.ms_since_process_start;
  S.profile = ready.data.profile;
  requestAnimationFrame(() => { renderCanvas(); renderReadout(); });
}
function reportRestore() {
  const r = S.view.restore;
  if (r.kind === 'restored') {
    const when = new Date(r.saved_at_unix * 1000).toISOString();
    notice(`Previous context restored (saved ${when}).${r.interrupted_jobs ? ` ${r.interrupted_jobs} job(s) were running at shutdown and are marked interrupted.` : ''}`);
  } else if (r.kind === 'unreadable') {
    notice(`Previous context could not be read: ${r.reason}. Defaults are in effect; the file was left in place.`, 'bad');
  }
}

// ── Render ────────────────────────────────────────────────────────────────
function renderAll() {
  renderCaseCard(); renderRail(); renderContext(); renderClock(); renderCanvas();
  renderInspector(); renderTimeline(); renderDock(); renderJobs(); renderReadout(); renderLocalFilter();
}

function renderCaseCard() {
  const c = $('case-card'); clear(c);
  const h = S.home;
  c.append(
    el('div', { class: 'k' }, `Case · ${h.title}`),
    el('div', { class: 'q' }, h.question),
    el('div', { class: 'chip-row' }, h.inventory.map((r) => badge({ glyph: r.origin.glyph, label: `${r.count} ${r.origin.label.toLowerCase()}${r.count === 1 ? '' : 's'}`, tone: r.origin.tone })),
      badge({ glyph: '…', label: `${h.open_questions.length} open`, tone: 'neutral' })),
  );
}

function renderRail() {
  const rail = $('rail'); clear(rail);
  const current = shell().instrument;
  const running = S.jobs.filter((j) => j.state === 'running' || j.state === 'queued').length;
  S.view.instruments.forEach((inst, i) => {
    if (inst.id === 'globe' || inst.id === 'jobs') rail.append(el('hr'));
    const extra = inst.id === 'jobs' && running ? `${running} running` : inst.id === 'report' && S.report ? `${S.report.uncited} uncited` : inst.reason;
    rail.append(el('button', {
      type: 'button', class: inst.available ? '' : 'off', 'aria-current': inst.id === current ? 'true' : null,
      'aria-disabled': inst.available ? null : 'true', 'aria-label': `${inst.label}${extra ? `, ${extra}` : ''} (Ctrl+${i + 1})`,
      onclick: () => switchInstrument(inst.id),
    }, el('span', { 'aria-hidden': 'true' }, inst.glyph), el('span', { class: 'tip', 'aria-hidden': 'true' }, inst.label, extra ? el('small', null, extra) : null)));
  });
}
function switchInstrument(id) {
  const inst = S.view.instruments.find((i) => i.id === id);
  if (!inst.available) { notice(`${inst.label}: ${inst.reason}.`); return; }
  applyShell(invoke('switch_instrument', { instrument: id }));
}

function renderContext() {
  const c = $('context'); clear(c);
  const ctx = shell().context; const s = shell().settings;
  const crumb = (label, value, onclick) => el('button', { type: 'button', class: 'crumb', onclick, title: `Open ${label.toLowerCase()}` }, el('span', null, label), el('b', null, value));
  const sep = () => el('span', { class: 'sep', 'aria-hidden': 'true' }, '›');
  c.append(
    crumb('Case', ctx.case_title, () => switchInstrument('case')), sep(),
    crumb('Area', ctx.area ? `${ctx.area.approximate ? '≈ ' : ''}${ctx.area.extent_km ? `${ctx.area.extent_km} km` : ctx.area.label}` : '— unknown', () => switchInstrument('case')), sep(),
    crumb('Interval', `${hhmm(ctx.interval.start_min)}–${hhmm(ctx.interval.end_min)}`, () => $('timeline').focus()), sep(),
    crumb('Selected', ctx.selection ? ctx.selection.object_id : 'none', () => { if (ctx.selection) setTab('selected'); }),
  );
  if (ctx.shared_filter) c.append(badge({ glyph: '⊂', label: `filter · ${ctx.shared_filter}`, tone: 'sel' }));
  if (s.clean_evidence) c.append(badge({ glyph: '◌', label: 'clean evidence', tone: 'sel' }));
  if (s.focused_reading) c.append(badge({ glyph: '▭', label: 'focused reading', tone: 'sel' }));
  c.append(el('button', { type: 'button', disabled: shell().route_stack.length === 0 ? true : null, onclick: routeBack, title: 'Alt+Left' }, `← Back · ${shell().route_stack.length}`));
}
function routeBack() { if (shell().route_stack.length) applyShell(invoke('route_back')); }

function renderClock() {
  const c = $('clock'); clear(c);
  const ctx = shell().context;
  const modeGlyph = { live: '●', historical: '▮', replay: '▶', simulated: '◌' }[ctx.clock_mode];
  c.append(
    el('div', { class: 'cell' }, el('span', { class: 'k' }, 'Mode'), el('span', { class: 'mode' }, el('span', { class: 'g', 'aria-hidden': 'true' }, modeGlyph), ctx.clock_mode.toUpperCase())),
    el('div', { class: 'cell' }, el('span', { class: 'k' }, 'Investigation clock'), el('span', { class: 'v big' }, hhmm(ctx.clock_min))),
    el('div', { class: 'cell ref' }, el('span', { class: 'k' }, 'Reference'), el('span', { class: 'v' }, `${ctx.interval.date} · ${ctx.interval.zone} · ${ctx.interval.reference}`)),
  );
}

// ── Canvas ────────────────────────────────────────────────────────────────
function renderCanvas() {
  const inst = shell().instrument;
  const canvas = $('canvas'); const pane = $('pane');
  clear(canvas);
  if (inst === 'case') { pane.hidden = true; drawGeometry(canvas); return; }
  $('unplotted').hidden = true;
  pane.hidden = false; clear(pane);
  ({ library: renderLibrary, report: renderReport, export: renderExport, jobs: renderJobsPane, settings: renderSettings })[inst]?.(pane);
}

// The part of the canvas no floating instrument covers; geometry is centred and scaled in it.
function freeRegion() {
  const w = $('canvas').clientWidth, h = $('canvas').clientHeight;
  const focused = shell().settings.focused_reading;
  return { left: focused ? 24 : 100, right: w - 384, top: 100, bottom: focused ? h - 86 : h - 150 };
}
function pxPerKm() {
  const f = freeRegion();
  return (Math.min(f.right - f.left, f.bottom - f.top) * 0.46) / S.geometry.region_radius_km;
}
function drawGeometry(canvas) {
  const g = S.geometry; const w = canvas.clientWidth, h = canvas.clientHeight;
  const f = freeRegion();
  const cx = (f.left + f.right) / 2, cy = (f.top + f.bottom) / 2, k = pxPerKm();
  const P = (p) => [cx + p.east * k, cy - p.north * k];
  const sel = shell().context.selection?.object_id;
  const clean = shell().settings.clean_evidence;
  canvas.setAttribute('viewBox', `0 0 ${w} ${h}`);
  const defs = svg('defs', null,
    svg('pattern', { id: 'hatch', width: 8, height: 8, patternUnits: 'userSpaceOnUse', patternTransform: 'rotate(45)' }, svg('line', { x1: 0, y1: 0, x2: 0, y2: 8, stroke: 'rgba(245,158,46,0.35)', 'stroke-width': 1 })),
    svg('marker', { id: 'arrow', viewBox: '0 0 10 10', refX: 9, refY: 5, markerWidth: 8, markerHeight: 8, orient: 'auto-start-reverse' }, svg('path', { d: 'M0 0L10 5L0 10z', fill: '#45d1ff' })),
    svg('radialGradient', { id: 'vig', cx: '50%', cy: '50%', r: '70%' }, svg('stop', { offset: '55%', 'stop-color': '#000', 'stop-opacity': 0 }), svg('stop', { offset: '100%', 'stop-color': '#000', 'stop-opacity': 0.75 })),
  );
  canvas.append(defs);

  // Overlays: rings, ticks, labels, brackets. Hidden by clean evidence.
  const overlay = svg('g', { class: 'overlay' });
  const rings = svg('g', { fill: 'none', 'stroke-width': 1 });
  for (const r of g.ring_km) {
    const edge = Math.abs(r - g.region_radius_km) < 1e-9;
    rings.append(svg('circle', { cx, cy, r: r * k, stroke: edge ? 'rgba(69,209,255,0.3)' : 'rgba(69,209,255,0.14)', 'stroke-dasharray': edge ? '3 6' : null }));
    overlay.append(svg('text', { x: cx - r * k - 6, y: cy - 4, 'text-anchor': 'end', fill: 'rgba(230,237,243,0.32)', 'font-size': 10, 'letter-spacing': 1.5 }, `${r} KM`));
  }
  overlay.append(rings);
  overlay.append(svg('text', { x: cx + g.region_radius_km * k * 0.75, y: cy + g.region_radius_km * k * 0.78, fill: 'rgba(230,237,243,0.32)', 'font-size': 10, 'letter-spacing': 1.5 }, `≈ ${g.region_radius_km} KM · EDGE OF REPORTED REGION`));
  const tick = svg('g', { stroke: 'rgba(69,209,255,0.35)', 'stroke-width': 1 });
  tick.append(svg('line', { x1: cx, y1: f.top - 30, x2: cx, y2: f.top }), svg('line', { x1: cx, y1: f.bottom, x2: cx, y2: f.bottom + 30 }), svg('line', { x1: f.left, y1: cy, x2: f.left + 40, y2: cy }), svg('line', { x1: f.right - 40, y1: cy, x2: f.right, y2: cy }));
  overlay.append(tick);
  const card = svg('g', { fill: 'rgba(230,237,243,0.55)', 'font-size': 10, 'letter-spacing': 2, 'text-anchor': 'middle' });
  card.append(svg('text', { x: cx + 14, y: f.top + 12 }, 'N'), svg('text', { x: cx + 14, y: f.bottom - 6 }, 'S'), svg('text', { x: f.left + 54, y: cy - 10 }, 'W'), svg('text', { x: f.right - 54, y: cy - 10 }, 'E'));
  overlay.append(card);
  overlay.append(svg('text', { x: f.left + 12, y: f.bottom - 10, fill: 'rgba(230,237,243,0.4)', 'font-size': 10, 'letter-spacing': 1.5 }, `REPORTED GEOMETRY · BASEMAP ${g.basemap.toUpperCase()} · ${g.scale_note.toUpperCase()}`));
  canvas.append(overlay);

  // Evidence: ridge sketch, regions, tracks, observers. Always drawn.
  const ev = svg('g', { class: 'evidence' });
  ev.append(svg('polyline', { points: g.ridge.map(P).map((p) => p.join(',')).join(' '), fill: 'none', stroke: 'rgba(230,237,243,0.5)', 'stroke-width': 1.5, 'stroke-dasharray': '6 5' }));
  const ridgeEnd = P(g.ridge[g.ridge.length - 1]);
  ev.append(svg('text', { class: 'overlay', x: ridgeEnd[0] + 8, y: ridgeEnd[1] - 8, fill: 'rgba(230,237,243,0.55)', 'font-size': 10, 'letter-spacing': 1.5 }, g.ridge_label.toUpperCase()));
  for (const r of g.regions) {
    const pts = r.polygon.map(P);
    const selected = r.account_id === sel;
    ev.append(objNode(r.account_id, svg('path', { d: `M${pts.map((p) => p.join(' ')).join(' L ')} Z`, fill: 'url(#hatch)', stroke: selected ? '#f59e2e' : 'rgba(245,158,46,0.6)', 'stroke-width': selected ? 1.5 : 1, 'stroke-dasharray': '4 4' }), `${r.label}, ${r.note}`));
    const lx = Math.max(...pts.map((p) => p[0])) - 160, ly = Math.max(...pts.map((p) => p[1])) + 20;
    ev.append(svg('g', { class: 'overlay', 'font-size': 10, 'letter-spacing': 1.5 }, svg('text', { x: lx, y: ly, fill: '#f59e2e' }, r.label.toUpperCase()), svg('text', { x: lx, y: ly + 14, fill: 'rgba(230,237,243,0.45)' }, r.note.toUpperCase())));
  }
  for (const t of g.tracks) {
    const a = P(t.from), b = P(t.to);
    const selected = t.account_id === sel;
    ev.append(objNode(t.account_id, svg('line', { x1: a[0], y1: a[1], x2: b[0], y2: b[1], stroke: '#45d1ff', 'stroke-width': selected ? 2 : 1.5, 'stroke-dasharray': '10 6', 'marker-end': 'url(#arrow)' }), `${t.label}, ${t.note}`));
    ev.append(svg('g', { class: 'overlay', 'font-size': 10, 'letter-spacing': 1.5 }, svg('text', { x: a[0] - 12, y: a[1] - 14, fill: '#45d1ff' }, t.label.toUpperCase()), svg('text', { x: a[0] - 12, y: a[1] }, svg('tspan', { fill: 'rgba(230,237,243,0.45)' }, t.note.toUpperCase()))));
  }
  for (const wdg of g.wedges) {
    const a = P(wdg.apex), l = P(wdg.left), r = P(wdg.right);
    ev.append(svg('path', { d: `M${a[0]} ${a[1]} L ${l[0]} ${l[1]} L ${r[0]} ${r[1]} Z`, fill: 'rgba(69,209,255,0.06)', stroke: 'rgba(69,209,255,0.35)', 'stroke-width': 1 }));
  }
  const unplotted = [];
  for (const o of g.observers) {
    if (o.position) {
      const [x, y] = P(o.position);
      const grp = svg('g', { transform: `translate(${x} ${y})` }, svg('circle', { r: 5, fill: '#45d1ff' }), svg('circle', { r: 11, fill: 'none', stroke: '#45d1ff', 'stroke-width': 1 }),
        svg('text', { class: 'overlay', x: 16, y: -6, fill: '#e6edf3', 'font-size': 10, 'letter-spacing': 1.5 }, o.label),
        svg('text', { class: 'overlay', x: 16, y: 7, fill: 'rgba(230,237,243,0.55)', 'font-size': 10, 'letter-spacing': 1.2 }, o.position_note.toUpperCase()));
      ev.append(objNode(o.account_id, grp, `Observer ${o.label}, ${o.position_note}`));
    } else {
      unplotted.push(o);
    }
  }
  // Unknown positions are listed outside the geometry, never placed at an invented point (design §7.6).
  const list = $('unplotted'); clear(list);
  list.hidden = unplotted.length === 0;
  if (unplotted.length) {
    list.append(el('span', { class: 'k' }, 'Not plotted'));
    for (const o of unplotted) {
      list.append(el('button', { type: 'button', 'aria-pressed': String(sel === o.account_id), class: sel === o.account_id ? 'primary' : '', onclick: () => selectObject(o.account_id) }, `${o.label} · ${o.position_note}`));
    }
  }
  ev.append(svg('g', { transform: `translate(${cx} ${cy})` }, svg('line', { x1: -8, y1: 0, x2: 8, y2: 0, stroke: '#45d1ff' }), svg('line', { x1: 0, y1: -8, x2: 0, y2: 8, stroke: '#45d1ff' })));
  canvas.append(ev);

  // Selection brackets around the selected object's actual geometry.
  if (sel && !clean) {
    const box = selectionBox(sel, P, cx, cy, k);
    if (box) {
      const [x1, y1, x2, y2] = box;
      canvas.append(svg('g', { class: 'overlay', fill: 'none', stroke: '#45d1ff', 'stroke-width': 1.2 },
        svg('path', { d: `M${x1} ${y1} h14 M${x1} ${y1} v14` }), svg('path', { d: `M${x2} ${y1} h-14 M${x2} ${y1} v14` }),
        svg('path', { d: `M${x1} ${y2} h14 M${x1} ${y2} v-14` }), svg('path', { d: `M${x2} ${y2} h-14 M${x2} ${y2} v-14` })));
    }
  }
  canvas.append(svg('g', { class: 'overlay', fill: 'none', stroke: 'rgba(69,209,255,0.28)', 'stroke-width': 1 }, svg('path', { d: `M${w * 0.28} 14 H ${w * 0.72} M ${w * 0.28} ${h - 14} H ${w * 0.72}` }), svg('path', { d: `M14 ${h * 0.33} V ${h * 0.67} M ${w - 14} ${h * 0.33} V ${h * 0.67}` })));
  canvas.append(svg('rect', { width: w, height: h, fill: 'url(#vig)', 'pointer-events': 'none' }));
}
function objNode(id, node, label) {
  node.classList.add('obj');
  node.setAttribute('tabindex', '0');
  node.setAttribute('role', 'button');
  node.setAttribute('aria-label', label);
  node.setAttribute('aria-pressed', String(shell().context.selection?.object_id === id));
  node.style.cursor = 'default';
  const go = () => selectObject(id);
  node.addEventListener('click', go);
  node.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); go(); } });
  return node;
}
function selectionBox(id, P, cx, cy, k) {
  const g = S.geometry; const pts = [];
  for (const t of g.tracks) if (t.account_id === id) pts.push(P(t.from), P(t.to));
  for (const r of g.regions) if (r.account_id === id) pts.push(...r.polygon.map(P));
  // An observer without a position contributes nothing: no invented point gets brackets.
  for (const o of g.observers) if (o.account_id === id && o.position) pts.push(P(o.position));
  if (!pts.length) return null;
  const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
  return [Math.min(...xs) - 20, Math.min(...ys) - 20, Math.max(...xs) + 20, Math.max(...ys) + 20];
}
async function selectObject(id) {
  const version = objectVersion(id);
  if (version === null) { notice(`Unknown object ${id}.`, 'bad'); return; }
  S.inspectorTab = 'selected';
  await applyShell(invoke('select_object', { objectId: id, version }));
}
function objectVersion(id) {
  if (id.startsWith('FIXTURE-ACC-')) return 1;
  if (id === 'FIXTURE-DOC-0007') return 2;
  if (id === 'FIXTURE-HYP-0001') return 2;
  if (id.startsWith('FIXTURE-CLM-')) return 1;
  return null;
}

// ── Panes ─────────────────────────────────────────────────────────────────
async function renderLibrary(pane) {
  if (!S.docs) S.docs = (await invoke('documents')).data;
  if (shell().instrument !== 'library') return;
  const lf = shell().local_filters.library || '';
  pane.append(el('h2', null, 'Library'));
  if (S.pendingSearchJob) {
    const job = S.jobs.find((j) => j.id === S.pendingSearchJob);
    if (job && (job.state === 'running' || job.state === 'queued')) pane.append(el('div', { class: 'coverage' }, `Searching… (${job.name}, ${job.id}; progress not reported)`));
  }
  if (S.search) {
    const r = S.search;
    pane.append(el('div', { class: 'section' },
      el('div', { class: 'chip-row' }, badge({ glyph: '⌕', label: `scope · ${r.scope}`, tone: 'sel' }), badge({ glyph: '=', label: `mode · ${r.mode}`, tone: 'neutral' }), badge({ glyph: '#', label: `${r.hits.length} result${r.hits.length === 1 ? '' : 's'}`, tone: r.outcome === 'empty' ? 'warn' : 'neutral' }), badge({ glyph: '¶', label: `page 1 of 1`, tone: 'neutral' })),
      el('div', { class: 'coverage' }, r.coverage),
      r.outcome === 'empty' ? el('div', { class: 'empty' }, `No passages contain "${r.query}". This is an empty result, not a failed search.`) : null,
      el('div', { class: 'list' }, r.hits.map((hit) => {
        const before = hit.text.slice(0, hit.match_start), m = hit.text.slice(hit.match_start, hit.match_end), after = hit.text.slice(hit.match_end);
        return el('div', { class: 'row obj hit', role: 'button', tabindex: 0, 'aria-selected': String(shell().context.selection?.object_id === hit.doc_id), onclick: () => selectObject(hit.doc_id), onkeydown: (e) => { if (e.key === 'Enter') selectObject(hit.doc_id); } },
          el('span', { class: 'title prose' }, before, el('mark', null, m), after), el('span', { class: 'v' }, `${hit.doc_id} · v${hit.version} · ${hit.anchor}`));
      })),
    ));
  }
  const docs = S.docs.filter((d) => !lf || d.title.toLowerCase().includes(lf.toLowerCase()) || d.id.toLowerCase().includes(lf.toLowerCase()));
  pane.append(el('div', { class: 'section' },
    el('div', { class: 'k' }, `Sources · ${docs.length} of ${S.docs.length}${lf ? ` · local filter "${lf}"` : ''}`),
    el('div', { class: 'list' }, docs.map((d) => el('div', { class: 'row obj', role: 'button', tabindex: 0, 'aria-selected': String(shell().context.selection?.object_id === d.id), onclick: () => selectObject(d.id), onkeydown: (e) => { if (e.key === 'Enter') selectObject(d.id); } },
      el('span', { class: 'title' }, d.title), el('span', { class: 'v' }, `${d.id} · v${d.version}`),
      el('span', { class: 'sub' }, badge(d.origin), badge(d.availability), badge(d.health), el('span', { class: 'k' }, `published ${d.publication}`), el('span', { class: 'k' }, d.extraction))))),
    el('div', { class: 'actions' }, el('button', { type: 'button', onclick: () => { const f = window.prompt('Local filter for Library (text in title or id):', lf); if (f !== null) applyShell(invoke('set_local_filter', { instrument: 'library', filter: f || null })); } }, lf ? 'Edit local filter' : 'Add local filter')),
  ));
}
async function renderReport(pane) {
  if (!S.report) { S.report = (await invoke('report_draft')).data; renderRail(); }
  if (shell().instrument !== 'report') return;
  const r = S.report;
  pane.append(el('h2', null, `Report · revision ${r.revision}`), el('div', { class: 'chip-row' }, badge({ glyph: '¶', label: `${r.statements.length} statements`, tone: 'neutral' }), badge({ glyph: '!', label: `${r.uncited} uncited`, tone: r.uncited ? 'warn' : 'ok' })));
  let section = null;
  for (const s of r.statements) {
    if (s.section !== section) { section = s.section; pane.append(el('div', { class: 'k' }, section)); }
    pane.append(el('div', { class: `stmt ${s.origin === 'generated' ? 'generated' : ''}` },
      s.citation ? el('button', { type: 'button', class: 'm', title: `Open ${s.citation}`, onclick: () => selectObject('FIXTURE-DOC-0007') }, '¹') : el('span', { class: 'm uncited', title: 'Uncited: add a citation or mark as an explicit assumption' }, '!'),
      el('span', { class: 'prose' }, s.text),
      el('span', { class: 'chip-row' }, s.origin === 'generated' ? badge({ glyph: '✦', label: s.reviewed ? 'generated · reviewed' : 'generated, unreviewed', tone: 'gen' }) : badge({ glyph: '◉', label: s.reviewed ? 'human · reviewed' : 'human · unreviewed', tone: 'neutral' }), s.citation ? el('span', { class: 'v' }, s.citation) : badge({ glyph: '!', label: 'uncited', tone: 'warn' }))));
  }
  pane.append(el('div', { class: 'actions' }, el('button', { type: 'button', class: 'primary', disabled: true, title: 'Editing is not implemented in this spike' }, 'Save · private'), el('button', { type: 'button', onclick: () => switchInstrument('export') }, 'Export…'), el('button', { type: 'button', disabled: true, title: 'Sharing is not implemented in this spike' }, 'Share…')));
}
async function renderExport(pane) {
  if (!S.exportPreview) S.exportPreview = (await invoke('export_preview')).data;
  if (shell().instrument !== 'export') return;
  const x = S.exportPreview;
  pane.append(el('h2', null, 'Export preview'), el('div', { class: 'coverage' }, x.note),
    el('div', { class: 'cols' },
      el('div', { class: 'card glass' }, el('div', { class: 'k' }, 'Formats'), el('div', { class: 'list' }, x.formats.map((f) => el('div', { class: 'row' }, el('span', { class: 'title' }, f.name), badge({ glyph: f.status === 'implemented' ? '●' : '○', label: f.status, tone: f.status === 'implemented' ? 'ok' : 'neutral' }), el('span', { class: 'sub k' }, f.note))))),
      el('div', { class: 'card glass' }, el('div', { class: 'k' }, 'Omissions and redactions'), el('div', { class: 'list' }, x.omissions.map((o) => el('div', { class: 'row' }, el('span', { class: 'prose', style: 'font-size:13px' }, o)))))),
    el('div', { class: 'actions' }, el('button', { type: 'button', class: 'primary', disabled: true, title: 'No exporter exists in this spike' }, 'Write file'), el('span', { class: 'k' }, 'Disabled: no implemented format')));
}
function renderJobsPane(pane) {
  pane.append(el('h2', null, 'Jobs'));
  const groups = [['Running', ['queued', 'running']], ['Needs attention', ['failed', 'interrupted', 'partially_succeeded']], ['Superseded', ['superseded']], ['Finished', ['succeeded', 'cancelled']]];
  for (const [title, states] of groups) {
    const jobs = S.jobs.filter((j) => states.includes(j.state));
    pane.append(el('div', { class: 'section' }, el('div', { class: 'k' }, `${title} · ${jobs.length}`), el('div', { class: 'list' }, jobs.length ? jobs.map((j) => jobRow(j, true)) : el('div', { class: 'empty' }, 'none'))));
  }
}
function renderSettings(pane) {
  const s = shell().settings;
  pane.append(el('h2', null, 'Settings'), el('div', { class: 'coverage' }, 'Each setting changes how the workspace is drawn. None of them changes data, selection, counts, or analysis results.'));
  const toggle = (key, label, effect, unchanged) => el('div', { class: 'setting' },
    el('input', { type: 'checkbox', id: `set-${key}`, checked: s[key] ? true : null, onchange: (e) => applyShell(invoke('set_setting', { change: { setting: key, value: e.target.checked } })) }),
    el('label', { for: `set-${key}` }, label), el('span', { class: 'k' }, `Effect: ${effect}`), el('span', { class: 'k' }, `Does not change: ${unchanged}`));
  pane.append(
    toggle('reduced_motion', 'Reduced motion', 'transitions become immediate; no pulses', 'data, selection behaviour'),
    toggle('reduced_transparency', 'Reduced transparency', 'opaque surfaces, no blur', 'layout, text, borders'),
    toggle('clean_evidence', 'Clean evidence (Ctrl+Shift+E)', 'hides overlays, annotations, brackets, ticks on the canvas', 'provenance labels on generated material, inspector'),
    toggle('focused_reading', 'Focused reading (Ctrl+Shift+F)', 'hides rail and timeline, widens the reading surface', 'shared context, route stack'),
    toggle('sound', 'Sound', 'off by default; nothing in this spike plays audio', 'visual notifications'),
  );
  const effects = el('select', { id: 'set-effects', onchange: (e) => applyShell(invoke('set_setting', { change: { setting: 'effects', value: e.target.value } })) }, ['low', 'standard', 'high'].map((v) => el('option', { value: v, selected: s.effects === v ? true : null }, v)));
  pane.append(el('div', { class: 'setting' }, effects, el('label', { for: 'set-effects' }, 'Effects quality'), el('span', { class: 'k' }, 'Effect: blur radius, shadow, grid'), el('span', { class: 'k' }, 'Does not change: numerical results, counts, analysis')));
  const px = el('input', { type: 'number', id: 'set-reading', min: 13, max: 24, value: s.reading_text_px, onchange: (e) => applyShell(invoke('set_setting', { change: { setting: 'reading_text_px', value: Number(e.target.value) } })) });
  pane.append(el('div', { class: 'setting' }, px, el('label', { for: 'set-reading' }, 'Reading text size (px)'), el('span', { class: 'k' }, 'Effect: scales reading surfaces only (13 to 24 px)'), el('span', { class: 'k' }, 'Does not change: instrument text')));
  pane.append(el('div', { class: 'k' }, `State file: ${S.view.store_path || '— not persisted'}`));
}

// ── Inspector ─────────────────────────────────────────────────────────────
const TABS = [['selected', 'Selected'], ['source', 'Source'], ['claims', 'Claims'], ['notes', 'Notes'], ['lineage', 'Lineage']];
function setTab(tab) { S.inspectorTab = tab; renderInspector(); }
function renderInspector() {
  const ins = $('inspector'); clear(ins);
  const tabs = el('div', { class: 'tabs', role: 'tablist' });
  TABS.forEach(([id, label], i) => tabs.append(el('button', { type: 'button', role: 'tab', id: `tab-${id}`, 'aria-selected': String(S.inspectorTab === id), tabindex: S.inspectorTab === id ? 0 : -1, title: `Ctrl+Alt+${i + 1}`, onclick: () => setTab(id),
    onkeydown: (e) => { const idx = TABS.findIndex(([t]) => t === S.inspectorTab); if (e.key === 'ArrowRight') { setTab(TABS[(idx + 1) % TABS.length][0]); $(`tab-${S.inspectorTab}`).focus(); } if (e.key === 'ArrowLeft') { setTab(TABS[(idx + TABS.length - 1) % TABS.length][0]); $(`tab-${S.inspectorTab}`).focus(); } } }, label)));
  ins.append(tabs);
  const body = el('div', { class: 'body', role: 'tabpanel' });
  ins.append(body);
  const o = S.object;
  if (!o) { body.append(el('div', { class: 'empty' }, 'Nothing selected. Select an account, source, or claim on the canvas or in a list.')); return; }
  switch (S.inspectorTab) {
    case 'selected': renderSelected(body, o); break;
    case 'source': renderSourceTab(body, o); break;
    case 'claims': renderClaimsTab(body, o); break;
    case 'notes': body.append(el('div', { class: 'k' }, 'Your notes'), el('div', { class: 'empty' }, (o.your_notes || []).length ? o.your_notes.join('\n') : 'No notes. Notes are separate records linked to the source version; the source stays byte-for-byte unchanged.'), el('div', { class: 'actions' }, el('button', { type: 'button', disabled: true, title: 'Not implemented in this spike' }, 'Add note'))); break;
    case 'lineage': body.append(el('div', { class: 'k' }, 'Lineage'), el('div', { class: 'list' }, (o.lineage || []).length ? o.lineage.map((l) => el('div', { class: 'v' }, l)) : el('div', { class: 'empty' }, o.type === 'source' ? 'No lineage recorded.' : 'Lineage is shown for sources. Open the anchored source to see its releases.'))); break;
  }
}
function kvRows(rows) {
  const kv = el('div', { class: 'kv' });
  for (const r of rows) kv.append(el('span', { class: 'k' }, r.key), el('span', null, el('span', { class: r.mono ? 'v' : 'sans' }, r.value)));
  return kv;
}
function renderSelected(body, o) {
  if (o.type === 'account') {
    const kv = el('div', { class: 'kv' });
    kv.append(el('span', { class: 'k' }, 'Object'), el('span', null, el('span', { class: 'v' }, o.label)),
      el('span', { class: 'k' }, 'Origin'), el('span', null, badge(o.origin)),
      el('span', { class: 'k' }, 'Anchor'), el('span', null, el('button', { type: 'button', class: 'v', style: 'padding:2px 6px', onclick: () => selectObject('FIXTURE-DOC-0007') }, o.anchor)),
      el('span', { class: 'k' }, 'Assessment'), el('span', null, badge(o.assessment), badge(o.review)),
      el('span', { class: 'k' }, 'Conflicts'), el('span', null, o.conflicts.length ? o.conflicts.map((c) => badge(c)) : el('span', { class: 'v dim' }, 'none')));
    body.append(kv, compassViz(o), timeCoverageViz(o));
    body.append(el('div', { class: 'k' }, 'Source record'), el('div', { class: 'source-record' }, kvRows(o.source_record)));
    body.append(el('div', { class: 'actions' }, el('button', { type: 'button', class: 'primary', disabled: true, title: 'Not implemented in this spike' }, 'Cite'), el('button', { type: 'button', disabled: true, title: 'Not implemented in this spike' }, 'Assess'), el('button', { type: 'button', onclick: () => selectObject('FIXTURE-HYP-0001') }, 'Discriminating evidence')));
  } else if (o.type === 'source') {
    renderSourceTab(body, o);
  } else if (o.type === 'hypothesis') {
    body.append(el('div', { class: 'kv' }, el('span', { class: 'k' }, 'Object'), el('span', null, el('span', { class: 'v' }, `${o.id} · v${o.version}`)), el('span', { class: 'k' }, 'Origin'), el('span', null, badge(o.origin))),
      el('div', { class: 'prose', style: 'font-size:14px' }, o.text),
      el('div', { class: 'k' }, `Supporting · ${o.supporting.length}`), el('div', { class: 'list' }, o.supporting.map((s) => el('div', { class: 'v' }, s))),
      el('div', { class: 'k' }, `Contradicting · ${o.contradicting.length}`), el('div', { class: 'list' }, o.contradicting.map((s) => el('div', { class: 'v' }, s))),
      el('div', { class: 'k' }, 'Evidence that would discriminate'), el('div', { class: 'sans' }, o.discriminating));
  } else if (o.type === 'claim') {
    body.append(el('div', { class: 'prose', style: 'font-size:14px' }, o.proposition),
      kvRows([{ key: 'Claimant', value: o.claimant, mono: false }, { key: 'Anchor', value: o.anchor, mono: true }, { key: 'Version', value: `${o.id} · v${o.version}`, mono: true }]),
      el('div', { class: 'chip-row' }, badge(o.assessment), badge(o.review)),
      el('div', { class: 'k' }, 'No probability is shown for a claim.'));
  }
}
function renderSourceTab(body, o) {
  if (o.type !== 'source') { body.append(el('div', { class: 'k' }, 'Anchored source'), el('div', { class: 'actions' }, el('button', { type: 'button', onclick: () => selectObject('FIXTURE-DOC-0007') }, 'Open FIXTURE-DOC-0007 · v2'))); return; }
  body.append(el('div', { class: 'kv' }, el('span', { class: 'k' }, 'Object'), el('span', null, el('span', { class: 'sans' }, o.title)), el('span', { class: 'k' }, 'Origin'), el('span', null, badge(o.origin)), el('span', { class: 'k' }, 'Availability'), el('span', null, badge(o.availability), badge(o.health))));
  body.append(pageCoverageViz(o));
  body.append(el('div', { class: 'k' }, 'Source record'), el('div', { class: 'source-record' }, kvRows(o.source_record)));
}
function renderClaimsTab(body) {
  const ids = ['FIXTURE-CLM-0001', 'FIXTURE-CLM-0002'];
  body.append(el('div', { class: 'k' }, `Claims anchored in this case · ${ids.length}`), el('div', { class: 'list' }, ids.map((id) => el('div', { class: 'row obj', role: 'button', tabindex: 0, 'aria-selected': String(shell().context.selection?.object_id === id), onclick: () => selectObject(id), onkeydown: (e) => { if (e.key === 'Enter') selectObject(id); } }, el('span', { class: 'v' }, id)))));
}
function compassViz(o) {
  const wrap = el('div', { class: 'viz' }, el('div', { class: 'head' }, el('span', { class: 'k' }, 'Reported direction of travel'), el('span', { class: 'k dim' }, o.label.slice(0, 9))));
  const b = o.bearing;
  const s = svg('svg', { viewBox: '0 0 284 110', width: 284, height: 110, role: 'img', 'aria-label': b ? `Compass: travel toward ${b.text}` : 'Compass: no bearing reported; drawn as a dashed full ring' });
  const g = svg('g', { transform: 'translate(55 55)' }, svg('circle', { r: 44, fill: 'none', stroke: 'rgba(255,255,255,0.12)' }), svg('circle', { r: 28, fill: 'none', stroke: 'rgba(255,255,255,0.08)' }), svg('text', { x: 0, y: -49, 'text-anchor': 'middle', 'font-size': 8, fill: 'rgba(230,237,243,0.55)' }, 'N'));
  if (b) {
    const half = b.plus_minus_degrees;
    const rad = (d) => (d - 90) * Math.PI / 180;
    const arc = (d) => [42 * Math.cos(rad(d)), 42 * Math.sin(rad(d))];
    const [lx, ly] = arc(b.degrees - half), [rx, ry] = arc(b.degrees + half), [tx, ty] = arc(b.degrees);
    g.append(svg('path', { d: `M0 0 L${lx} ${ly} A42 42 0 0 1 ${rx} ${ry} Z`, fill: 'rgba(69,209,255,0.18)', stroke: '#45d1ff', 'stroke-width': 1 }), svg('line', { x1: 0, y1: 0, x2: tx, y2: ty, stroke: '#45d1ff', 'stroke-width': 1.5 }));
  } else {
    g.append(svg('circle', { r: 36, fill: 'none', stroke: '#f59e2e', 'stroke-width': 1, 'stroke-dasharray': '2 5' }));
  }
  g.append(svg('circle', { r: 2, fill: '#e6edf3' }));
  s.append(g, svg('g', { 'font-size': 8, 'letter-spacing': 1, fill: 'rgba(230,237,243,0.7)' }, svg('text', { x: 120, y: 50 }, b ? b.text : 'no bearing reported'), svg('text', { x: 120, y: 66, fill: 'rgba(230,237,243,0.4)' }, b ? `${b.degrees}° ± ${b.plus_minus_degrees}° (reported)` : 'absence drawn as dashed ring')));
  wrap.append(s);
  return wrap;
}
function timeCoverageViz(o) {
  const t = o.times;
  const wrap = el('div', { class: 'viz' }, el('div', { class: 'head' }, el('span', { class: 'k' }, 'Coverage'), el('span', { class: 'k dim' }, 'known · unknown')));
  const s = svg('svg', { viewBox: '0 0 284 36', width: 284, height: 36, role: 'img', 'aria-label': `Event time ${t.event}, received ${t.received}, position ${t.position}` });
  const cols = [['EVENT', t.event, 46], ['RECEIVED', t.received, 136], ['POSITION', t.position, 216]];
  for (const [label, cov, x] of cols) {
    s.append(svg('text', { x: x + 24, y: 8, 'text-anchor': 'middle', 'font-size': 7, fill: 'rgba(230,237,243,0.4)' }, label));
    const attrs = { x, y: 14, width: 48, height: 5, rx: 2 };
    if (cov === 'approximate') Object.assign(attrs, { fill: '#45d1ff' });
    else if (cov === 'recorded') Object.assign(attrs, { fill: '#5fe08a' });
    else if (cov === 'region_only') Object.assign(attrs, { fill: 'url(#hatch)', stroke: 'rgba(245,158,46,0.6)' });
    else Object.assign(attrs, { fill: 'none', stroke: 'rgba(255,255,255,0.25)', 'stroke-dasharray': '2 2' });
    s.append(svg('rect', attrs), svg('text', { x: x + 24, y: 30, 'text-anchor': 'middle', 'font-size': 7, fill: 'rgba(230,237,243,0.55)' }, cov.replace('_', ' ')));
  }
  wrap.append(s, el('div', { class: 'legend' }, el('span', null, el('i', { style: 'background:#45d1ff' }), 'approximate'), el('span', null, el('i', { style: 'background:#5fe08a' }), 'recorded'), el('span', null, el('i', { style: 'background:transparent;border:1px dashed rgba(255,255,255,0.4)' }), 'not recorded'), el('span', null, el('i', { style: 'background:rgba(245,158,46,0.5)' }), 'region only')));
  return wrap;
}
function pageCoverageViz(o) {
  const wrap = el('div', { class: 'viz' }, el('div', { class: 'head' }, el('span', { class: 'k' }, 'Extraction coverage'), el('span', { class: 'k dim' }, `${o.pages - o.unreadable_pages.length} of ${o.pages} pages`)));
  const s = svg('svg', { viewBox: `0 0 284 30`, width: 284, height: 30, role: 'img', 'aria-label': `${o.pages} pages; unreadable: ${o.unreadable_pages.join(', ')}` });
  for (let p = 1; p <= o.pages; p++) {
    const bad = o.unreadable_pages.includes(p);
    s.append(svg('rect', { x: (p - 1) * 30, y: 4, width: 24, height: 12, rx: 2, fill: bad ? 'url(#hatch)' : '#5fe08a', stroke: bad ? 'rgba(245,158,46,0.6)' : 'none' }), svg('text', { x: (p - 1) * 30 + 12, y: 27, 'text-anchor': 'middle', 'font-size': 7, fill: 'rgba(230,237,243,0.55)' }, bad ? `${p} ✕` : String(p)));
  }
  wrap.append(s);
  return wrap;
}

// ── Timeline ──────────────────────────────────────────────────────────────
function renderTimeline() {
  const t = $('timeline'); clear(t);
  const ctx = shell().context;
  const { start_min: a, end_min: b } = ctx.interval;
  t.append(el('div', { class: 'left' }, el('span', { class: 'k' }, 'Shared timeline'), el('span', { class: 'v' }, `${hhmm(a)} → ${hhmm(b)}`), el('span', { class: 'k dim' }, `${ctx.interval.date} · ← → scrub`)));
  const W = 760;
  const X = (m) => ((m - a) / (b - a)) * W;
  const s = svg('svg', { viewBox: `0 0 ${W} 70`, width: '100%', height: 70, preserveAspectRatio: 'none', role: 'img', 'aria-label': `Timeline ${hhmm(a)} to ${hhmm(b)}, clock at ${hhmm(ctx.clock_min)}` });
  s.append(svg('line', { x1: 0, y1: 44, x2: W, y2: 44, stroke: 'rgba(255,255,255,0.2)' }));
  for (let m = a; m <= b; m += 60) s.append(svg('line', { x1: X(m), y1: 40, x2: X(m), y2: 48, stroke: 'rgba(255,255,255,0.2)' }), svg('text', { x: Math.min(X(m) + 2, W - 38), y: 62, 'font-size': 10, fill: 'rgba(230,237,243,0.45)', 'letter-spacing': 1 }, hhmm(m)));
  const absent = [];
  for (const mk of S.marks) {
    const selected = ctx.selection?.object_id === mk.account_id;
    if (mk.range_min) {
      const [m0, m1] = mk.range_min;
      s.append(svg('rect', { x: X(m0), y: 38, width: X(m1) - X(m0), height: 12, rx: 3, fill: selected ? 'rgba(69,209,255,0.3)' : 'rgba(69,209,255,0.18)', stroke: 'rgba(69,209,255,0.6)' }),
        svg('text', { x: X((m0 + m1) / 2), y: 30, 'text-anchor': 'middle', 'font-size': 10, fill: '#45d1ff', 'letter-spacing': 1 }, `${mk.glyph} ${mk.label} · ${mk.kind.toUpperCase()} ${mk.note.replace('event ', '').toUpperCase()}`));
    } else absent.push(mk.label);
  }
  if (absent.length) s.append(svg('text', { x: W - 4, y: 30, 'text-anchor': 'end', 'font-size': 10, fill: 'rgba(230,237,243,0.4)', 'letter-spacing': 1 }, `${absent.join(', ')} · NO EVENT TIME`));
  if (ctx.timeline_focus) { const [f0, f1] = ctx.timeline_focus; s.append(svg('rect', { class: 'overlay', x: X(f0), y: 10, width: X(f1) - X(f0), height: 48, fill: 'none', stroke: 'rgba(69,209,255,0.35)', 'stroke-dasharray': '3 3' })); }
  s.append(svg('line', { x1: X(ctx.clock_min), y1: 14, x2: X(ctx.clock_min), y2: 56, stroke: '#45d1ff', 'stroke-width': 1.5 }));
  t.append(s);
  t.onkeydown = (e) => {
    const step = e.shiftKey ? 50 : 5;
    if (e.key === 'ArrowLeft') { e.preventDefault(); applyShell(invoke('scrub_clock', { deltaMin: -step })); }
    if (e.key === 'ArrowRight') { e.preventDefault(); applyShell(invoke('scrub_clock', { deltaMin: step })); }
    if (e.key === 'Home') { e.preventDefault(); applyShell(invoke('set_clock', { clockMin: a })); }
    if (e.key === 'End') { e.preventDefault(); applyShell(invoke('set_clock', { clockMin: b })); }
  };
}

// ── Dock, jobs, readout, notices ──────────────────────────────────────────
function renderDock() {
  $('scope-chip').textContent = `Scope · this case`;
}
$('dock').addEventListener('submit', async (e) => {
  e.preventDefault();
  const q = $('command').value.trim();
  if (!q) return;
  try {
    S.search = null;
    const env = await invoke('start_search', { query: q, scope: 'this case' });
    S.pendingSearchJob = env.data.job_id;
    const [jobs, view] = await Promise.all([invoke('list_jobs'), invoke('shell_state')]);
    S.jobs = jobs.data; S.view = view.data;
    if (shell().instrument !== 'library') await applyShell(invoke('switch_instrument', { instrument: 'library' })); else renderAll();
  } catch (err) { notice(errText(err), 'bad'); }
});
$('save-btn').addEventListener('click', () => notice('Save is private and keeps this case local. Nothing is saved by the spike beyond the shell context, which is already persisted.'));

async function onJobsChanged() {
  // Refresh the shell view too: the query sequence lives in Rust and may have advanced.
  const [jobs, view] = await Promise.all([invoke('list_jobs'), invoke('shell_state')]);
  S.jobs = jobs.data; S.view = view.data;
  for (const j of S.jobs) {
    if (j.kind.kind !== 'search') continue;
    if (j.id === S.pendingSearchJob && j.state === 'succeeded' && j.applied && j.kind.query_seq === shell().query_seq) {
      S.search = j.result; S.pendingSearchJob = null;
    } else if (j.state === 'superseded' && !j.applied && !S.dismissedNotices.has(j.id)) {
      notice(`A result for a previous query finished (${j.name}). It was not applied.`, '', [
        ['Open', async () => { const r = await invoke('open_job_result', { jobId: j.id }); S.search = r.data; S.pendingSearchJob = null; S.jobs = (await invoke('list_jobs')).data; hideNotice(); renderAll(); }],
        ['Dismiss', () => { S.dismissedNotices.add(j.id); hideNotice(); }],
      ]);
    }
  }
  renderAll();
}
function jobRow(j, expanded) {
  const glyph = { queued: '…', running: '⟳', succeeded: '✓', partially_succeeded: '◩', failed: '✕', cancelled: '⊘', superseded: '⤳', interrupted: '‖' }[j.state];
  const tone = { failed: 'bad', interrupted: 'warn', partially_succeeded: 'warn', superseded: 'warn' }[j.state] || '';
  const row = el('div', { class: 'job' }, el('span', { class: 'name', title: j.name }, j.name));
  const btns = el('span', { class: 'btns' });
  if (j.state === 'queued' || j.state === 'running') btns.append(el('button', { type: 'button', onclick: () => jobAction('cancel_job', j.id) }, 'Cancel'));
  if (['failed', 'cancelled', 'interrupted'].includes(j.state)) btns.append(el('button', { type: 'button', disabled: j.attempt >= j.max_attempts ? true : null, title: j.attempt >= j.max_attempts ? 'All attempts used' : '', onclick: () => jobAction('retry_job', j.id) }, 'Retry'));
  if (['succeeded', 'partially_succeeded', 'superseded'].includes(j.state)) btns.append(el('button', { type: 'button', onclick: async () => { const r = await invoke('open_job_result', { jobId: j.id }); if (j.kind.kind === 'search') { S.search = r.data; S.pendingSearchJob = null; if (shell().instrument !== 'library') await applyShell(invoke('switch_instrument', { instrument: 'library' })); } else notice(`${j.name}: ${j.extent || 'result opened'} (${JSON.stringify(r.data)})`); S.jobs = (await invoke('list_jobs')).data; renderAll(); } }, 'Open'));
  row.append(btns);
  if (j.state === 'running') {
    if (j.progress) { const bar = el('div', { class: 'bar' }, el('i')); bar.firstChild.style.width = `${(100 * j.progress.done) / j.progress.total}%`; row.append(bar); }
    else row.append(el('div', { class: 'bar indeterminate', 'aria-label': 'progress not reported' }));
  }
  const parts = [`${glyph} ${j.state.replace('_', ' ')}`];
  if (j.progress && (j.state === 'running' || j.state === 'queued')) parts.push(`${j.progress.done} / ${j.progress.total} ${j.progress.unit}`);
  else if (j.state === 'running') parts.push('progress not reported');
  if (j.extent) parts.push(j.extent);
  if (j.error) parts.push(j.error);
  if (j.state === 'superseded') parts.push(j.applied ? 'opened by user' : 'not applied');
  if (expanded) parts.push(`${j.id} · attempt ${j.attempt}/${j.max_attempts} · target ${j.target}`);
  if (j.fixture) parts.push('fixture');
  row.append(el('span', { class: `sub ${tone}` }, parts.join(' · ')));
  return row;
}
async function jobAction(cmd, id) {
  try { S.jobs = (await invoke(cmd, { jobId: id })).data; renderAll(); } catch (e) { notice(errText(e), 'bad'); }
}
function renderJobs() {
  const tray = $('jobs'); clear(tray);
  const counts = {}; for (const j of S.jobs) counts[j.state] = (counts[j.state] || 0) + 1;
  tray.append(el('div', { class: 'k' }, `Jobs · ${Object.entries(counts).map(([s, n]) => `${n} ${s.replace('_', ' ')}`).join(' · ') || 'none'}`));
  const shown = S.jobs.filter((j) => j.state !== 'succeeded' || !j.applied).slice(-4);
  for (const j of shown) tray.append(jobRow(j, false));
}
function renderReadout() {
  const r = $('readout'); clear(r);
  const ctx = shell().context; const s = shell().settings;
  const rows = [['View', `${shell().instrument}${shell().instrument === 'case' ? ' · geometry' : ''}`], ['Basemap', shell().instrument === 'case' ? S.geometry.basemap : '—'], ['Scale', shell().instrument === 'case' ? `≈ ${pxPerKm().toFixed(0)} px / km` : '—'], ['Selected', ctx.selection ? ctx.selection.object_id.replace('FIXTURE-', '') : 'none'], ['Overlays', s.clean_evidence ? 'off' : 'on'], ['Clock', hhmm(ctx.clock_min)]];
  if (S.startupMs !== null) rows.push(['Start', `${S.startupMs} ms · ${S.profile}`]);
  if (S.view.persist_error) rows.push(['Persist', 'FAILED']);
  for (const [k, v] of rows) r.append(el('div', { class: 'r' }, el('span', { class: 'k' }, k), el('span', { class: 'v' }, v)));
  if (S.view.persist_error) notice(`Shell context could not be saved: ${S.view.persist_error}`, 'bad');
}
function renderLocalFilter() {
  const box = $('local-filter'); clear(box);
  const inst = shell().instrument; const lf = shell().local_filters[inst];
  const divergent = S.view.divergent_local_filters.includes(inst);
  if (!lf || !divergent) { box.hidden = true; return; }
  box.hidden = false;
  box.append(badge({ glyph: '⊂', label: `local filter · ${lf}`, tone: 'warn' }),
    el('button', { type: 'button', onclick: () => applyShell(invoke('promote_local_filter', { instrument: inst })) }, 'Promote to shared'),
    el('button', { type: 'button', onclick: () => applyShell(invoke('set_local_filter', { instrument: inst, filter: null })) }, 'Clear'));
}
let noticeTimer = null;
function notice(text, tone, actions) {
  const n = $('notice'); clear(n); n.hidden = false; n.className = `notice glass strong ${tone || ''}`;
  n.append(el('span', null, text));
  for (const [label, fn] of actions || []) n.append(el('button', { type: 'button', onclick: fn }, label));
  n.append(el('button', { type: 'button', 'aria-label': 'Dismiss notice', onclick: hideNotice }, '✕'));
  clearTimeout(noticeTimer);
  if (!actions) noticeTimer = setTimeout(hideNotice, 8000);
}
function hideNotice() { $('notice').hidden = true; }

// ── Keyboard model (design §8.1) ──────────────────────────────────────────
const REGIONS = ['rail', 'context', 'canvas', 'inspector', 'timeline', 'dock', 'jobs'];
function focusRegion(dir) {
  const active = document.activeElement;
  let idx = REGIONS.findIndex((id) => $(id).contains(active) || ($(id).id === 'canvas' && $('pane').contains(active)));
  idx = (idx + dir + REGIONS.length) % REGIONS.length;
  const region = $(REGIONS[idx]);
  const target = REGIONS[idx] === 'canvas' && !$('pane').hidden ? $('pane') : region;
  const first = target.querySelector('button:not(:disabled), input, select, [tabindex="0"], [role="tab"][tabindex="0"]');
  (first || target).focus();
}
document.addEventListener('keydown', (e) => {
  const inText = e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT' || e.target.isContentEditable);
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); $('command').focus(); $('command').select(); return; }
  if (!inText && e.key === '/') { e.preventDefault(); $('command').focus(); return; }
  if (mod && !e.shiftKey && !e.altKey && /^[0-9]$/.test(e.key)) { e.preventDefault(); const inst = S.view.instruments[e.key === '0' ? 9 : Number(e.key) - 1]; if (inst) switchInstrument(inst.id); return; }
  if (mod && e.shiftKey && (e.key === ']' || e.key === '[' || e.key === '}' || e.key === '{')) {
    e.preventDefault();
    const avail = S.view.instruments.filter((i) => i.available);
    const idx = avail.findIndex((i) => i.id === shell().instrument);
    const next = avail[(idx + (e.key === ']' || e.key === '}' ? 1 : avail.length - 1)) % avail.length];
    switchInstrument(next.id); return;
  }
  if (mod && e.altKey && /^[1-5]$/.test(e.key)) { e.preventDefault(); setTab(TABS[Number(e.key) - 1][0]); $(`tab-${S.inspectorTab}`).focus(); return; }
  if (e.altKey && e.key === 'ArrowLeft') { e.preventDefault(); routeBack(); return; }
  if (e.key === 'F6') { e.preventDefault(); focusRegion(e.shiftKey ? -1 : 1); return; }
  if (mod && e.shiftKey && e.key.toLowerCase() === 'e') { e.preventDefault(); applyShell(invoke('set_setting', { change: { setting: 'clean_evidence', value: !shell().settings.clean_evidence } })); return; }
  if (mod && e.shiftKey && e.key.toLowerCase() === 'f') { e.preventDefault(); applyShell(invoke('set_setting', { change: { setting: 'focused_reading', value: !shell().settings.focused_reading } })); return; }
  if (e.key === 'Escape') { if (inText) e.target.blur(); hideNotice(); }
});

boot().catch((e) => { document.body.append(el('div', { class: 'notice glass strong bad', style: 'display:flex' }, `Startup failed: ${errText(e)}`)); });
