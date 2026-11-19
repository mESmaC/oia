'use strict';
// Presentation only: Rust owns bytes, extraction, search, cleanup and publication.
const $ = id => document.getElementById(id);
const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
let busy = false, documentView = null, imageView = null, searchView = null;
let page = 0, selectedLine = null;
let activeTab = 'home';
const pageSize = 100;
const targets = {
  text: ['Text metadata', 'PNG text containers'],
  modification_time: ['Modification time', 'PNG tIME container'],
  exif: ['Exif metadata', 'May include location, device and capture details'],
  comment: ['Comments', 'JPEG comment containers'],
  xmp: ['XMP metadata', 'Supported XMP packets'],
  photoshop_irb: ['Photoshop metadata', 'Supported image resource containers']
};
const handlingLabels = { retained: 'Retained', removable: 'Removable', required_for_rendering: 'Required for rendering', unsupported: 'Unsupported' };

function node(tag, text, className) {
  const element = document.createElement(tag);
  if (text !== undefined) element.textContent = text;
  if (className) element.className = className;
  return element;
}
function showTab(name, focus = false) {
  if (!['home', 'documents', 'privacy'].includes(name)) name = 'home';
  activeTab = name;
  document.querySelectorAll('.view').forEach(view => { view.hidden = view.id !== name; });
  document.querySelectorAll('[data-tab]').forEach(button => {
    if (button.dataset.tab === name) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  });
  if (focus) $('main').focus();
  renderChrome();
}
function renderChrome() {
  const names = { home: 'WORKSPACE', documents: 'DOCUMENTS', privacy: 'FILE PRIVACY' };
  const selected = activeTab === 'documents' ? documentView : activeTab === 'privacy' ? imageView : null;
  $('active-view').textContent = names[activeTab];
  $('readout-view').textContent = names[activeTab].toLowerCase();
  $('active-source').textContent = selected?.file.name || 'NONE SELECTED';
  $('readout-source').textContent = selected ? '1 selected' : 'none';
  const count = Number(!!documentView) + Number(!!imageView);
  $('loaded-count').textContent = `${count} ${count === 1 ? 'FILE' : 'FILES'} LOADED`;
  $('home-canvas-title').textContent = count ? 'Source selection' : 'No source loaded';
  $('home-canvas-prompt').textContent = count ? 'Select a loaded source in the session inspector.' : 'Open a document or image from the command dock.';
  $('session-sources').replaceChildren();
  for (const [name, view] of [['documents', documentView], ['privacy', imageView]]) {
    if (!view) continue;
    const button = node('button', view.file.name);
    button.append(node('small', `${view.file.bytes.toLocaleString()} bytes · session snapshot`));
    button.onclick = () => showTab(name, true);
    $('session-sources').append(button);
  }
  if (!count) $('session-sources').append(node('p', 'No files in this session.', 'small'));
  $('extent-fill').hidden = !selected || (activeTab === 'privacy' && !imageView.complete);
  if (activeTab === 'documents' && documentView) {
    const bytes = searchView ? searchView.searched_bytes : documentView.extracted_bytes;
    const complete = searchView ? searchView.complete : documentView.complete;
    $('extent-label').textContent = `${searchView ? 'SEARCH' : 'EXTRACTION'} / ${complete ? 'COMPLETE' : 'PARTIAL'}`;
    $('extent-detail').textContent = `${bytes.toLocaleString()} / ${documentView.file.bytes.toLocaleString()} B`;
    $('extent-fill').value = documentView.file.bytes ? bytes / documentView.file.bytes * 100 : 100;
  } else if (activeTab === 'privacy' && imageView) {
    $('extent-label').textContent = imageView.complete ? 'SUPPORTED PROFILE' : 'PARTIAL / UNSUPPORTED REGIONS';
    $('extent-detail').textContent = `${imageView.rows.length} CONTAINERS`;
    $('extent-fill').value = imageView.complete ? 100 : 0;
  } else {
    $('extent-label').textContent = 'No source selected'; $('extent-detail').textContent = '—'; $('extent-fill').value = 0;
  }
  $('query').placeholder = documentView ? `Search ${documentView.file.name} · exact text…` : 'Open a document to search…';
  for (const id of ['query', 'search-submit', 'clear-search']) $(id).disabled = busy || !documentView;
}
function setBusy(value, message) {
  busy = value;
  document.querySelectorAll('[data-operation]').forEach(control => { control.disabled = value; });
  $('cancel').hidden = !value;
  $('cancel').disabled = false;
  $('status').textContent = message;
  updateExport();
  renderChrome();
}
async function operation(command, args, message, apply) {
  if (busy) return;
  $('error').hidden = true;
  setBusy(true, message);
  let status = 'Ready · Original files remain unchanged.';
  try {
    const response = await invoke(command, args);
    if (response.schema !== 'oia-local-tools/1') throw { message: 'This application response uses an unsupported version.' };
    if (response.data === null) status = 'File dialog closed · Previous file retained.';
    else status = apply(response.data);
  } catch (error) {
    status = error?.code === 'cancelled' ? 'Cancelled · Previous file retained.' : 'Operation failed · Previous file retained.';
    $('error-message').textContent = error?.message || String(error);
    $('error').hidden = false;
  } finally { setBusy(false, status); }
}
function details(element, info, extra = []) {
  element.replaceChildren();
  for (const [key, value] of [['File', info.name], ['Original location', info.path], ['Loaded bytes', info.bytes.toLocaleString()], ...extra, ['SHA-256', info.sha256]]) {
    element.append(node('dt', key), node('dd', value, key === 'SHA-256' ? 'hash' : ''));
  }
}
function openDocument() {
  operation('open_document', {}, 'Choose a UTF-8 file · Reading begins after selection…', data => {
    documentView = data; searchView = null; page = 0; selectedLine = null;
    $('query').value = ''; $('search-panel').hidden = true;
    $('document-empty').hidden = true; $('document-loaded').hidden = false;
    $('document-name').textContent = data.file.name;
    details($('document-details'), data.file);
    $('document-coverage').textContent = `${data.complete ? 'Complete for this file' : 'Partial — passage limit reached'} · ${data.extracted_bytes.toLocaleString()} of ${data.file.bytes.toLocaleString()} bytes extracted; ${data.passages.length.toLocaleString()} passages. Limit: ${data.passage_limit.toLocaleString()} passages.`;
    $('document-coverage').classList.toggle('partial', !data.complete);
    renderPassages(); showTab('documents');
    return `Loaded ${data.file.name} · ${data.complete ? 'Complete text extraction' : 'Partial text extraction'} · Session only`;
  });
}
function renderPassages() {
  if (!documentView) return;
  const totalPages = Math.max(1, Math.ceil(documentView.passages.length / pageSize));
  page = Math.max(0, Math.min(page, totalPages - 1));
  const fragment = document.createDocumentFragment();
  for (const passage of documentView.passages.slice(page * pageSize, (page + 1) * pageSize)) {
    const row = node('div', undefined, `passage${selectedLine === passage.line ? ' selected' : ''}`);
    row.id = `line-${passage.line}`;
    row.tabIndex = -1;
    row.title = `Line ${passage.line} · bytes ${passage.start}–${passage.end} (end exclusive)`;
    row.setAttribute('aria-label', row.title);
    row.append(node('span', String(passage.line), 'line-number'), node('pre', passage.text, 'source-text'));
    fragment.append(row);
  }
  if (!documentView.passages.length) fragment.append(node('p', 'This file is empty.', 'padded'));
  $('passages').replaceChildren(fragment);
  $('page-label').textContent = `Page ${page + 1} of ${totalPages} · ${pageSize} passages per page`;
  $('previous-page').disabled = page === 0;
  $('next-page').disabled = page + 1 === totalPages;
  $('passages').scrollTop = 0;
}
function renderSearch() {
  $('search-panel').hidden = false;
  $('search-count').textContent = `${searchView.matches.length} ${searchView.matches.length === 1 ? 'MATCH' : 'MATCHES'}`;
  $('search-coverage').textContent = `${searchView.complete ? 'Complete search of this file' : 'Partial search — a search or extraction limit was reached'}. Searched ${searchView.searched_bytes.toLocaleString()} of ${searchView.input_bytes.toLocaleString()} bytes for “${searchView.query}”. Up to ${searchView.match_limit} matches; exact, case-sensitive, within lines.`;
  $('search-coverage').classList.toggle('partial', !searchView.complete);
  $('matches').replaceChildren();
  for (const match of searchView.matches) {
    const button = node('button', `Line ${match.line} · bytes ${match.start}–${match.end} · ${match.text}`, 'match');
    button.onclick = () => {
      selectedLine = match.line; page = Math.floor((match.line - 1) / pageSize); renderPassages();
      const line = $(`line-${match.line}`);
      line?.scrollIntoView({ block: 'center' }); line?.focus({ preventScroll: true });
    };
    $('matches').append(button);
  }
  if (!searchView.matches.length) $('matches').append(node('p', 'No matches in the searched bytes. This says nothing about other files or sources.', 'small'));
}
function openImage() {
  operation('open_image', {}, 'Choose a PNG or JPEG · Inspecting begins after selection…', data => {
    imageView = data;
    $('privacy-empty').hidden = true; $('privacy-loaded').hidden = false; $('export-result').hidden = true;
    $('image-name').textContent = data.file.name;
    details($('image-details'), data.file, [['Dimensions', `${data.width} × ${data.height} pixels`], ['Container profile', data.profile]]);
    $('privacy-coverage').textContent = data.complete ? 'Complete within the supported container profile. This is not a claim that the file contains no identifying information.' : `Partial inspection. Unsupported container or image parameters${data.trailing_bytes ? `; ${data.trailing_bytes} bytes after the image end` : ''}. Cleanup is unavailable.`;
    $('privacy-coverage').classList.toggle('partial', !data.complete);
    const fragment = document.createDocumentFragment();
    for (const row of data.rows) {
      const tr = node('tr');
      tr.append(node('td', row.kind), node('td', `${row.start}–${row.end}`), node('td', handlingLabels[row.handling], `handling-${row.handling}`));
      fragment.append(tr);
    }
    $('metadata').replaceChildren(fragment);
    $('selections').replaceChildren();
    for (const [key, [title, description]] of Object.entries(targets)) {
      const rows = data.rows.filter(row => row.target === key);
      if (!rows.length) continue;
      const label = node('label', undefined, 'selection'), input = node('input');
      input.type = 'checkbox'; input.name = key;
      input.disabled = !data.complete || rows.some(row => row.handling === 'required_for_rendering');
      input.onchange = updateExport;
      const text = node('span', title);
      text.append(node('small', input.disabled ? 'Retained — cleanup unavailable for this category' : description));
      label.append(input, text); $('selections').append(label);
    }
    if (!$('selections').children.length) $('selections').append(node('p', 'No removable metadata categories found.', 'small'));
    updateExport(); showTab('privacy');
    return `Inspected ${data.file.name} · ${data.complete ? 'Supported container profile' : 'Partial coverage — cleanup unavailable'}`;
  });
}
function updateExport() {
  const selected = [...$('selections').querySelectorAll('input')].some(input => input.checked && !input.disabled);
  $('export-cleaned').disabled = busy || !imageView?.complete || !selected;
  $('cleanup-reason').textContent = !imageView ? '' : !imageView.complete ? 'An unsupported region prevents a verified cleanup.' : !selected ? 'Select a removable category to create a cleaned copy.' : 'The saved copy is reinspected. Existing files cannot be overwritten.';
}

document.querySelectorAll('[data-tab]').forEach(button => { button.onclick = () => showTab(button.dataset.tab, true); });
document.querySelector('.brand').onclick = event => { event.preventDefault(); showTab('home', true); };
$('home-document').onclick = $('open-document').onclick = openDocument;
$('home-image').onclick = $('open-image').onclick = openImage;
$('search-form').onsubmit = event => {
  event.preventDefault(); if (!documentView) return;
  operation('search_document', { query: $('query').value, sha256: documentView.file.sha256 }, 'Searching the loaded snapshot…', data => {
    searchView = data; renderSearch(); showTab('documents');
    return `${data.matches.length} ${data.matches.length === 1 ? 'match' : 'matches'} · ${data.complete ? 'Complete search of this file' : 'Partial search; see coverage'}`;
  });
};
$('query').oninput = () => { searchView = null; $('search-panel').hidden = true; selectedLine = null; renderPassages(); renderChrome(); };
$('clear-search').onclick = () => { $('query').value = ''; searchView = null; $('search-panel').hidden = true; selectedLine = null; renderPassages(); renderChrome(); $('query').focus(); };
$('previous-page').onclick = () => { page--; renderPassages(); };
$('next-page').onclick = () => { page++; renderPassages(); };
$('export-cleaned').onclick = () => {
  if (!imageView) return;
  const selection = {};
  $('selections').querySelectorAll('input').forEach(input => { selection[input.name] = input.checked && !input.disabled; });
  operation('export_cleaned', { sha256: imageView.file.sha256, selection }, 'Choose a new filename · Cleanup and reinspection follow…', data => {
    const result = $('export-result'); result.hidden = false;
    result.replaceChildren(node('strong', 'Cleaned copy saved'), node('p', data.path), node('p', `${data.removed} metadata containers removed · ${data.bytes.toLocaleString()} bytes. Original unchanged.`), node('p', `Copy SHA-256: ${data.sha256}`, 'hash'));
    return `Saved cleaned copy · ${data.removed} containers removed and output reinspected`;
  });
};
$('cancel').onclick = async () => {
  $('cancel').disabled = true;
  try { await invoke('cancel_operation'); $('status').textContent = 'Cancellation requested · Close any open file dialog to finish.'; }
  catch (error) { $('error-message').textContent = error?.message || String(error); $('error').hidden = false; }
};
$('dismiss-error').onclick = () => { $('error').hidden = true; };
for (const [id, className] of [['effects', 'reduced'], ['focus', 'focused']]) {
  $(id).onclick = () => { const enabled = document.body.classList.toggle(className); $(id).setAttribute('aria-pressed', String(enabled)); };
}
document.addEventListener('keydown', event => {
  if (event.key === 'F6') {
    event.preventDefault();
    const regions = [document.querySelector('.rail'), $('main'), document.querySelector('.view:not([hidden]) .inspector'), document.querySelector('.command-dock'), document.querySelector('.statusbar')].filter(region => region && region.getClientRects().length);
    let index = regions.findIndex(region => region.contains(document.activeElement));
    index = (index + (event.shiftKey ? -1 : 1) + regions.length) % regions.length;
    const target = regions[index]; target.tabIndex = -1; target.focus();
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f' && documentView) { event.preventDefault(); showTab('documents'); $('query').focus(); }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'o') { event.preventDefault(); if (!busy) openDocument(); }
  if (event.key === 'Escape') $('error').hidden = true;
});
if (window.matchMedia('(prefers-reduced-transparency: reduce)').matches) { document.body.classList.add('reduced'); $('effects').setAttribute('aria-pressed', 'true'); }
showTab('home');
