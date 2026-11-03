'use strict';
// Presentation only: Rust owns bytes, file operations, globe state and witness capture transitions.
// This file draws responses from the `oia-local-tools/1` commands and sends intents. Local state
// here is presentation state (active instrument, reading page, selected row, inspector tab).
const $ = id => document.getElementById(id);
const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
const SCHEMA = 'oia-local-tools/1';
const PAGE_SIZE = 100;
const INSTRUMENTS = ['session', 'documents', 'witness', 'privacy', 'globe'];
const TARGETS = {
  text: ['Text metadata', 'PNG text containers'],
  modification_time: ['Modification time', 'PNG tIME container'],
  exif: ['Exif metadata', 'May include location, device and capture details'],
  comment: ['Comments', 'JPEG comment containers'],
  xmp: ['XMP metadata', 'Supported XMP packets'],
  photoshop_irb: ['Photoshop metadata', 'Supported image resource containers']
};
const HANDLING = { retained: 'Retained', removable: 'Removable', required_for_rendering: 'Required for rendering', unsupported: 'Unsupported' };

let busy = false, documentView = null, imageView = null, searchView = null;
let instrument = 'session', page = 0, selectedLine = null, selectedRow = null;

const fmt = n => Number(n).toLocaleString('en-US');
const pad2 = n => String(n).padStart(2, '0');
function node(tag, text, className) {
  const el = document.createElement(tag);
  if (text !== undefined) el.textContent = text;
  if (className) el.className = className;
  return el;
}
function tag(number, label) {
  const el = node('button', undefined, 'tag');
  el.type = 'button';
  el.append(node('i', pad2(number)), node('b', label));
  return el;
}
function setText(id, text, className) {
  const el = $(id);
  el.textContent = text;
  if (className !== undefined) el.className = className;
}
function meter(id, done, total, complete) {
  // A meter is drawn only for a bounded quantity with a printed numerator and denominator.
  const bar = $(id);
  bar.replaceChildren();
  const segments = 10;
  const lit = total > 0 ? Math.round(done / total * segments) : (complete ? segments : 0);
  for (let i = 0; i < segments; i++) {
    const seg = node('i');
    if (i < lit) seg.className = complete ? 'on' : 'part';
    bar.append(seg);
  }
}

// ---- Shell: instrument switching, readout, rail, crumb ----
function showInstrument(name, focus = false) {
  instrument = INSTRUMENTS.includes(name) ? name : 'session';
  for (const n of INSTRUMENTS) {
    $(`view-${n}`).hidden = n !== instrument;
    $(`insp-${n}`).hidden = n !== instrument;
  }
  document.querySelectorAll('[data-instrument]').forEach(button => {
    if (button.dataset.instrument === instrument) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  });
  document.querySelectorAll('.globe-tool').forEach(button => { button.hidden = instrument !== 'globe'; });
  if (instrument === 'globe') Globe.activate(); else Globe.deactivate();
  if (instrument === 'witness') Witness.activate(); else Witness.deactivate();
  if (focus) (instrument === 'globe' ? $('globe-canvas') : $('canvas')).focus();
  renderShell();
}
function currentView() {
  return instrument === 'documents' ? documentView : instrument === 'privacy' ? imageView : null;
}
function renderShell() {
  const names = { session: '', documents: 'Documents', witness: 'Witness', privacy: 'Privacy', globe: 'Globe' };
  const view = currentView();
  $('crumb-sep').hidden = $('crumb-instrument').hidden = instrument === 'session';
  $('crumb-instrument').textContent = names[instrument];
  const loaded = Number(!!documentView) + Number(!!imageView);
  const witnessLoaded = Witness.summary().loaded;
  const sessionRecords = loaded + Number(witnessLoaded);
  $('crumb-chip').textContent = sessionRecords ? `${sessionRecords} session ${sessionRecords === 1 ? 'record' : 'records'} · no case` : 'No case · memory only';

  setText('ro-source', view ? view.file.name : '—', view ? 'v' : 'v none');
  setText('ro-hash', view ? view.file.sha256.slice(0, 8) + '…' : '—', view ? 'v' : 'v none');
  if (instrument === 'documents' && documentView) {
    const complete = searchView ? searchView.complete : documentView.complete;
    const what = searchView ? 'search' : 'extraction';
    setText('ro-coverage', `${what} ${complete ? 'complete' : 'partial'}`, complete ? 'v' : 'v est');
    setText('ro-selection', selectedLine === null ? 'none' : `line ${selectedLine}`, selectedLine === null ? 'v none' : 'v');
  } else if (instrument === 'privacy' && imageView) {
    setText('ro-coverage', imageView.complete ? 'profile supported' : 'partial', imageView.complete ? 'v' : 'v est');
    const row = selectedRow === null ? null : imageView.rows[selectedRow];
    setText('ro-selection', row ? `${pad2(selectedRow + 1)} ${row.kind} · b${row.start}–${row.end}` : 'none', row ? 'v' : 'v none');
  } else if (instrument === 'witness') {
    const w = Witness.summary();
    setText('ro-source', w.source, w.loaded ? 'v' : 'v none');
    setText('ro-hash', '—', 'v none');
    setText('ro-coverage', w.coverage, 'v est');
    setText('ro-selection', w.selection, w.loaded ? 'v' : 'v none');
  } else if (instrument === 'globe') {
    const g = Globe.summary();
    setText('ro-source', g.source, g.loaded ? 'v' : 'v none');
    setText('ro-hash', '—', 'v none');
    setText('ro-coverage', g.coverage, g.partial ? 'v est' : g.loaded ? 'v' : 'v none');
    setText('ro-selection', g.selection, g.selection === 'none' || g.selection === '—' ? 'v none' : 'v');
  } else {
    setText('ro-coverage', '—', 'v none');
    setText('ro-selection', loaded ? `${loaded} loaded` : '—', 'v none');
  }

  const scope = $('scope');
  scope.textContent = documentView ? 'THIS DOCUMENT' : 'NO DOCUMENT';
  scope.classList.toggle('off', !documentView);
  $('query').placeholder = documentView ? `Exact text in ${documentView.file.name}` : 'Open a document to search exact text';
  for (const id of ['query', 'search-submit', 'clear-search']) $(id).disabled = busy || !documentView;

  // Session instrument
  $('session-title').textContent = sessionRecords ? 'Records in this session' : 'No session records';
  $('session-prompt').textContent = sessionRecords ? 'Select a record to open its instrument. Nothing is saved; files carry no recorded position or time, and witness text is not normalized into measurements.' : 'Open a UTF-8 document or PNG/JPEG image, or capture a witness account. Everything stays in memory for this session.';
  const tray = $('session-tray'), list = $('session-sources');
  tray.replaceChildren(); list.replaceChildren();
  let n = 0;
  for (const [name, v] of [['documents', documentView], ['privacy', imageView]]) {
    if (!v) continue;
    n += 1;
    for (const host of [tray, list]) {
      const t = tag(n, `${v.file.name} · ${fmt(v.file.bytes)} B`);
      t.onclick = () => showInstrument(name, true);
      host.append(t);
    }
  }
  if (witnessLoaded) {
    n += 1;
    const w = Witness.summary();
    for (const host of [tray, list]) {
      const t = tag(n, `${w.source} · ${w.selection}`);
      t.onclick = () => showInstrument('witness', true);
      host.append(t);
    }
  }
  if (!n) list.append(node('p', 'No records in this session.'));
}

// ---- Operations: one worker, cancellable, publication only on success ----
function setBusy(value, message, cls) {
  busy = value;
  document.querySelectorAll('[data-operation]').forEach(control => { control.disabled = value; });
  $('cancel').hidden = !value;
  $('cancel').disabled = false;
  setText('status', message, `count ${cls || (value ? 'busy' : '')}`);
  updateExport();
  renderShell();
}
async function operation(command, args, message, apply) {
  if (busy) return;
  $('error').hidden = true;
  setBusy(true, message);
  let status = 'Idle · original files unchanged', cls = '';
  try {
    const response = await invoke(command, args);
    if (response.schema !== SCHEMA) throw { message: 'This application response uses an unsupported version.' };
    if (response.data === null) status = 'Idle · file dialog closed · previous file retained';
    else status = apply(response.data);
  } catch (error) {
    cls = 'bad';
    status = error?.code === 'cancelled' ? 'Cancelled · previous file retained' : 'Failed · previous file retained';
    $('error-message').textContent = error?.message || String(error);
    $('error').hidden = false;
  } finally { setBusy(false, status, cls); }
}
function details(element, info, extra = []) {
  element.replaceChildren();
  const rows = [['File', info.name], ['Path', info.path], ['Bytes', fmt(info.bytes)], ...extra, ['SHA-256', info.sha256], ['Published', null], ['Event time', null]];
  for (const [key, value] of rows) {
    element.append(node('dt', key), node('dd', value === null ? '— not recorded' : value, value === null ? 'none' : ''));
  }
}

// ---- Documents ----
function openDocument() {
  operation('open_document', {}, 'Choose a UTF-8 file · reading begins after selection', data => {
    documentView = data; searchView = null; page = 0; selectedLine = null;
    $('query').value = ''; $('search-panel').hidden = true;
    $('document-empty').hidden = true; $('document-loaded').hidden = false;
    $('document-name').textContent = data.file.name;
    $('insp-doc-name').textContent = data.file.name;
    details($('document-details'), data.file);
    renderDocumentCoverage();
    renderPassages();
    showInstrument('documents');
    return `Loaded ${data.file.name} · ${data.complete ? 'complete' : 'partial'} extraction · session only`;
  });
}
function renderDocumentCoverage() {
  if (!documentView) return;
  const d = documentView;
  setText('m-extract-n', `${fmt(d.extracted_bytes)} / ${fmt(d.file.bytes)} B`);
  meter('m-extract', d.extracted_bytes, d.file.bytes, d.complete);
  setText('m-passages-n', `${fmt(d.passages.length)} / ${fmt(d.passage_limit)} limit`);
  meter('m-passages', d.passages.length, d.passage_limit, true);
  $('m-search-wrap').hidden = !searchView;
  if (searchView) {
    setText('m-search-n', `${fmt(searchView.searched_bytes)} / ${fmt(searchView.input_bytes)} B`);
    meter('m-search', searchView.searched_bytes, searchView.input_bytes, searchView.complete);
  }
  setText('document-coverage', `${d.complete ? 'Complete for this file.' : 'Partial: the passage limit was reached; remaining bytes were not extracted.'} ${fmt(d.passages.length)} passages from ${fmt(d.extracted_bytes)} of ${fmt(d.file.bytes)} bytes.`, d.complete ? 'coverage' : 'coverage partial');
}
function renderPassages() {
  if (!documentView) return;
  const total = Math.max(1, Math.ceil(documentView.passages.length / PAGE_SIZE));
  page = Math.max(0, Math.min(page, total - 1));
  const fragment = document.createDocumentFragment();
  for (const passage of documentView.passages.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE)) {
    const row = node('div', undefined, `passage${selectedLine === passage.line ? ' selected' : ''}`);
    row.id = `line-${passage.line}`;
    row.tabIndex = -1;
    row.setAttribute('aria-label', `Line ${passage.line}, bytes ${passage.start} to ${passage.end} end exclusive`);
    row.append(node('span', String(passage.line), 'ln'), node('pre', passage.text));
    fragment.append(row);
  }
  if (!documentView.passages.length) fragment.append(node('p', 'This file is empty.', 'passage empty-file'));
  $('passages').replaceChildren(fragment);
  $('page-label').textContent = `Page ${page + 1} / ${total} · ${PAGE_SIZE} passages per page`;
  $('previous-page').disabled = page === 0;
  $('next-page').disabled = page + 1 === total;
  $('passages').scrollTop = 0;
}
function renderSearch() {
  const s = searchView;
  $('search-panel').hidden = false;
  setText('search-count', `${s.matches.length} ${s.matches.length === 1 ? 'match' : 'matches'} · “${s.query}”`);
  setText('search-chip', s.complete ? 'Complete search' : 'Partial search', s.complete ? 'chip' : 'chip warn');
  setText('search-coverage', `Searched ${fmt(s.searched_bytes)} of ${fmt(s.input_bytes)} bytes${s.complete ? '' : ' before a limit was reached'}. Exact, case-sensitive, within lines, up to ${s.match_limit} matches.${s.matches.length ? '' : ' No matches in the searched bytes; this says nothing about other files.'}`, s.complete ? 'coverage' : 'coverage partial');
  const list = $('matches');
  list.replaceChildren();
  s.matches.forEach((m, i) => {
    const b = node('button', undefined, 'match');
    b.type = 'button';
    b.append(node('span', pad2(i + 1), 'n'), node('span', `L${m.line} · b${m.start}–${m.end}`, 'at'), node('span', m.text, 't'));
    if (selectedLine === m.line) b.setAttribute('aria-current', 'true');
    b.onclick = () => {
      selectedLine = m.line; page = Math.floor((m.line - 1) / PAGE_SIZE);
      renderPassages(); renderSearch(); renderShell();
      const line = $(`line-${m.line}`);
      line?.scrollIntoView({ block: 'center' }); line?.focus({ preventScroll: true });
    };
    list.append(b);
  });
}
function clearSearch(focusQuery) {
  searchView = null; selectedLine = null;
  $('search-panel').hidden = true;
  renderPassages(); renderDocumentCoverage(); renderShell();
  if (focusQuery) $('query').focus();
}

// ---- Privacy ----
function openImage() {
  operation('open_image', {}, 'Choose a PNG or JPEG · inspection begins after selection', data => {
    imageView = data; selectedRow = null;
    $('privacy-empty').hidden = true; $('privacy-loaded').hidden = false; $('export-result').hidden = true;
    $('insp-img-name').textContent = data.file.name;
    details($('image-details'), data.file, [['Dimensions', `${data.width} × ${data.height} px`], ['Profile', data.profile]]);
    setText('bytemap-total', fmt(data.file.bytes));
    setText('privacy-chip', data.complete ? 'Supported profile' : 'Partial · unsupported region', data.complete ? 'chip ok' : 'chip warn');
    setText('privacy-coverage', data.complete
      ? 'Complete within the supported container profile. Not a claim that the file holds no identifying information: dimensions, density and image content remain.'
      : `Partial inspection: unsupported container or image parameters${data.trailing_bytes ? `; ${fmt(data.trailing_bytes)} bytes after the image end` : ''}. Cleanup is unavailable.`,
      data.complete ? 'coverage' : 'coverage partial');
    renderByteMap();
    renderSelections();
    showInstrument('privacy');
    return `Inspected ${data.file.name} · ${data.complete ? 'supported container profile' : 'partial coverage, cleanup unavailable'}`;
  });
}
function renderByteMap() {
  const v = imageView, total = v.file.bytes || 1;
  const bar = $('bytebar'), body = $('metadata');
  bar.replaceChildren(); body.replaceChildren();
  v.rows.forEach((row, i) => {
    const seg = node('span', undefined, `seg ${row.handling}${selectedRow === i ? ' selected' : ''}`);
    // Positions come from recorded byte ranges; CSSOM assignment is permitted by the CSP, inline style attributes are not.
    seg.style.left = `${row.start / total * 100}%`;
    seg.style.width = `${Math.max((row.end - row.start) / total * 100, 0.2)}%`;
    seg.title = `${pad2(i + 1)} ${row.kind} · bytes ${row.start}–${row.end} · ${HANDLING[row.handling]}`;
    seg.onclick = () => selectRow(i);
    bar.append(seg);
    const tr = node('tr', undefined, selectedRow === i ? 'selected' : '');
    tr.append(node('td', pad2(i + 1), 'num'), node('td', row.kind), node('td', `${row.start}–${row.end}`), node('td', HANDLING[row.handling], `h-${row.handling}`));
    tr.tabIndex = 0;
    tr.onclick = () => selectRow(i);
    tr.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); selectRow(i); } };
    body.append(tr);
  });
  if (!v.rows.length) {
    const tr = node('tr'); const td = node('td', 'No metadata containers were detected in this file.'); td.colSpan = 4; tr.append(td); body.append(tr);
  }
}
function selectRow(i) {
  selectedRow = selectedRow === i ? null : i;
  renderByteMap(); renderShell();
}
function renderSelections() {
  const v = imageView, host = $('selections');
  host.replaceChildren();
  for (const [key, [title, description]] of Object.entries(TARGETS)) {
    const rows = v.rows.filter(row => row.target === key);
    if (!rows.length) continue;
    const label = node('label', undefined, 'selection'), input = node('input');
    input.type = 'checkbox'; input.name = key;
    input.disabled = !v.complete || rows.some(row => row.handling === 'required_for_rendering');
    input.onchange = updateExport;
    const text = node('span', `${title} · ${rows.length}`);
    text.append(node('small', input.disabled ? 'Retained: cleanup unavailable for this category' : description));
    if (input.disabled) label.classList.add('off');
    label.append(input, text); host.append(label);
  }
  if (!host.children.length) host.append(node('p', 'No removable metadata categories found.', 'note'));
  updateExport();
}
function updateExport() {
  const selected = [...$('selections').querySelectorAll('input')].some(input => input.checked && !input.disabled);
  $('export-cleaned').disabled = busy || !imageView?.complete || !selected;
  $('cleanup-reason').textContent = !imageView ? '' : !imageView.complete ? 'An unsupported region prevents a verified cleanup.' : !selected ? 'Select a removable category to create a cleaned copy.' : 'The saved copy is reinspected before it is published. Existing files are never overwritten.';
}

// ---- Wiring ----
document.querySelectorAll('[data-instrument]').forEach(button => { button.onclick = () => showInstrument(button.dataset.instrument, true); });
document.querySelectorAll('[role="tablist"]').forEach(list => {
  const tabs = [...list.querySelectorAll('[role="tab"]')];
  const panel = list.nextElementSibling;
  const select = i => {
    tabs.forEach((t, j) => t.setAttribute('aria-selected', String(i === j)));
    panel.querySelectorAll('[data-pane]').forEach(p => { p.hidden = p.dataset.pane !== tabs[i].dataset.tab; });
    tabs[i].focus();
  };
  tabs.forEach((t, i) => {
    t.onclick = () => select(i);
    t.onkeydown = e => {
      const map = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1 };
      if (e.key in map) { e.preventDefault(); select((map[e.key] + tabs.length) % tabs.length); }
    };
  });
});
$('open-document').onclick = openDocument;
$('open-image').onclick = openImage;
$('search-form').onsubmit = event => {
  event.preventDefault();
  if (!documentView) return;
  operation('search_document', { query: $('query').value, sha256: documentView.file.sha256 }, 'Searching the loaded snapshot', data => {
    searchView = data; selectedLine = null;
    renderSearch(); renderPassages(); renderDocumentCoverage();
    showInstrument('documents');
    return `${data.matches.length} ${data.matches.length === 1 ? 'match' : 'matches'} · ${data.complete ? 'complete search' : 'partial search, see coverage'}`;
  });
};
const QUERY_BYTE_LIMIT = 512; // the Rust side caps the query in UTF-8 bytes, not characters
$('query').oninput = () => {
  if (searchView) clearSearch(false);
  const bytes = new TextEncoder().encode($('query').value).length;
  const over = bytes > QUERY_BYTE_LIMIT;
  $('search-submit').disabled = busy || !documentView || over;
  if (over) setText('status', `Query is ${fmt(bytes)} bytes; the limit is ${QUERY_BYTE_LIMIT} UTF-8 bytes`, 'count bad');
};
$('clear-search').onclick = () => { $('query').value = ''; clearSearch(true); };
$('previous-page').onclick = () => { page--; renderPassages(); };
$('next-page').onclick = () => { page++; renderPassages(); };
$('export-cleaned').onclick = () => {
  if (!imageView) return;
  const selection = {};
  $('selections').querySelectorAll('input').forEach(input => { selection[input.name] = input.checked && !input.disabled; });
  operation('export_cleaned', { sha256: imageView.file.sha256, selection }, 'Choose a new filename · cleanup and reinspection follow', data => {
    const result = $('export-result'); result.hidden = false;
    result.replaceChildren(node('span', 'Cleaned copy saved', 'k'), node('div', data.path), node('div', `${data.removed} containers removed · ${fmt(data.bytes)} bytes · original unchanged`), node('div', `copy SHA-256 ${data.sha256}`));
    return `Saved cleaned copy · ${data.removed} containers removed · output reinspected`;
  });
};
$('cancel').onclick = async () => {
  $('cancel').disabled = true;
  try { await invoke('cancel_operation'); setText('status', 'Cancellation requested · close any open file dialog to finish', 'count busy'); }
  catch (error) { $('error-message').textContent = error?.message || String(error); $('error').hidden = false; }
};
$('dismiss-error').onclick = () => { $('error').hidden = true; };
function setPresentation(className, on) {
  // Presentation only: never touches a value. Focus hides the rail and inspector, so an exit
  // control stays visible in the canvas tools and Escape leaves Focus (review B1).
  document.body.classList.toggle(className, on);
  const toggle = className === 'reduced' ? $('effects') : $('focus');
  toggle.setAttribute('aria-pressed', String(on));
  $('vs-overlays').textContent = document.body.classList.contains('reduced') ? 'reduced' : 'on';
  if (className === 'focused') $('exit-focus').hidden = !on;
  if (className === 'reduced') Globe.setReduced(on);
}
$('effects').onclick = () => setPresentation('reduced', !document.body.classList.contains('reduced'));
$('focus').onclick = () => setPresentation('focused', !document.body.classList.contains('focused'));
$('exit-focus').onclick = () => { setPresentation('focused', false); $('focus').focus(); };
document.addEventListener('keydown', event => {
  if (event.key === 'F6') {
    event.preventDefault();
    const regions = [document.querySelector('.rail'), $('canvas'), $('inspector'), document.querySelector('.jobs')].filter(r => r && r.getClientRects().length);
    let index = regions.findIndex(r => r.contains(document.activeElement));
    index = (index + (event.shiftKey ? -1 : 1) + regions.length) % regions.length;
    regions[index].tabIndex = -1; regions[index].focus();
  }
  const key = event.key.toLowerCase();
  if ((event.ctrlKey || event.metaKey) && key === 'f' && documentView) { event.preventDefault(); showInstrument('documents'); $('query').focus(); }
  if ((event.ctrlKey || event.metaKey) && key === 'o') { event.preventDefault(); if (!busy) openDocument(); }
  if ((event.ctrlKey || event.metaKey) && key === 'i') { event.preventDefault(); if (!busy) openImage(); }
  if ((event.ctrlKey || event.metaKey) && key === 'g') { event.preventDefault(); showInstrument('globe', true); }
  if ((event.ctrlKey || event.metaKey) && event.shiftKey && key === 'e' && instrument === 'globe') { event.preventDefault(); Globe.toggleOverlays(); }
  if (event.key === 'Escape') {
    if (!$('error').hidden) $('error').hidden = true;
    else if (document.body.classList.contains('focused')) { setPresentation('focused', false); $('focus').focus(); }
  }
});
Globe.onView(() => { if (instrument === 'globe') renderShell(); });
Witness.onView(() => renderShell());
if (window.matchMedia('(prefers-reduced-transparency: reduce)').matches) setPresentation('reduced', true);
showInstrument('session');
