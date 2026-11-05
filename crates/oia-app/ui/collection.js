'use strict';

// Presentation over the shared Rust registry. Opening an instrument starts acquisition;
// read-only collection_view/original never contact a source. Source markup is text only.
const PublicCollection = (() => {
  const panels = new Map();
  let active = null;
  let timer = null;
  let running = false;
  let busyListener = null;
  let workspaceBusy = null;
  let viewListener = null;
  function safe(value) {
    return Array.from(String(value), character => {
      const code = character.codePointAt(0);
      const control = code < 32 || (code >= 127 && code <= 159);
      const format = code === 0x85 || code === 0xad || (code >= 0x600 && code <= 0x605) ||
        code === 0x61c || code === 0x6dd || code === 0x70f || (code >= 0x890 && code <= 0x891) ||
        code === 0x8e2 || code === 0x180e || (code >= 0x200b && code <= 0x200f) ||
        (code >= 0x2028 && code <= 0x202e) || (code >= 0x2060 && code <= 0x206f) ||
        code === 0x3164 || code === 0xfeff || code === 0xffa0 || (code >= 0xfff9 && code <= 0xfffb) ||
        code === 0x110bd || code === 0x110cd || (code >= 0x13430 && code <= 0x1343f) ||
        (code >= 0x1bca0 && code <= 0x1bca3) || (code >= 0x1d173 && code <= 0x1d17a) ||
        (code >= 0xe0000 && code <= 0xe007f);
      const hidden = code !== 9 && code !== 10 &&
        (control || (format && code !== 0x200c && code !== 0x200d));
      return hidden ? String.fromCharCode(92) + 'u{' + code.toString(16).toUpperCase() + '}' : character;
    }).join('');
  }

  const el = (tag, text, cls) => {
    const result = document.createElement(tag);
    if (text !== undefined) result.textContent = safe(text);
    if (cls) result.className = cls;
    return result;
  };
  const date = ms => ms === null ? 'Never' : new Date(ms).toISOString();
  const request = async (command, args) => {
    const reply = await window.__TAURI__.core.invoke(command, args);
    if (reply?.schema !== 'oia-local-tools/1') throw new Error('Unsupported source-collection response.');
    return reply.data;
  };
  function button(text, action) {
    const result = el('button', text, 'btn');
    result.type = 'button';
    result.onclick = action;
    return result;
  }
  function labelled(text, input) {
    const label = el('label', text);
    label.append(input);
    return label;
  }
  function status(panel, message) { panel.status.textContent = safe(message); }

  function mount(category) {
    const host = document.getElementById('collection-' + category);
    const panel = { host, category, view: null, started: false, query: '', selected: null, generation: 0 };
    host.className = 'public-collection';
    host.append(el('span', 'Public sources / internet collection', 'k'), el('h2', category === 'news' ? 'News from the source.' : 'Explore published UFO-report records.'));
    host.append(el('p', category === 'news'
      ? 'Attributed news feeds load automatically. Read the supplied excerpts, inspect their source versions, then add your own material below.'
      : 'Search The National Archives’ historical UFO-report catalogue. These are file descriptions and references, not full witness testimony or verified sightings. Add a personal account below.', 'note'));
    const actions = el('div', undefined, 'news-actions');
    panel.refresh = button('Refresh public sources', () => refresh(category));
    panel.refresh.classList.add('primary');
    panel.cancel = button('Cancel collection', async () => {
      if (!running) return;
      try { await request('cancel_operation', {}); status(panel, 'Cancellation requested. Previous source responses stay retained.'); }
      catch (error) { status(panel, error.message ?? String(error)); }
    });
    panel.cancel.disabled = true;
    panel.auto = el('input'); panel.auto.type = 'checkbox'; panel.auto.checked = true;
    panel.auto.onchange = schedule;
    actions.append(panel.refresh, panel.cancel, labelled('Refresh every 15 minutes while this tab is open', panel.auto));
    host.append(actions);
    if (category === 'reports') {
      const search = el('form', undefined, 'news-filter glass');
      panel.archiveQuery = el('input'); panel.archiveQuery.type = 'search'; panel.archiveQuery.maxLength = 120; panel.archiveQuery.placeholder = 'e.g. Rendlesham Forest';
      const submit = el('button', 'Search archive', 'btn'); submit.type = 'submit';
      search.append(labelled('Archive keywords · UFO is always included', panel.archiveQuery), submit);
      search.onsubmit = event => { event.preventDefault(); panel.query = panel.archiveQuery.value; refresh(category); };
      host.append(search);
    }
    const filters = el('div', undefined, 'news-filter glass');
    panel.filter = el('input'); panel.filter.type = 'search'; panel.filter.placeholder = 'Search these collected titles and excerpts'; panel.filter.maxLength = 256;
    panel.source = el('select'); panel.source.append(el('option', 'All public sources')); panel.source.firstElementChild.value = '';
    panel.filter.oninput = () => renderEntries(panel); panel.source.onchange = () => renderEntries(panel);
    filters.append(labelled('Filter retained results', panel.filter), labelled('Source', panel.source));
    host.append(filters);
    panel.status = el('p', 'Opening this tab retrieves its registered public sources.', 'coverage'); panel.status.setAttribute('role', 'status');
    panel.sources = el('div', undefined, 'collection-sources');
    panel.counts = el('p', undefined, 'note');
    panel.results = el('div', undefined, 'collection-columns');
    panel.list = el('ul', undefined, 'news-list');
    panel.reader = el('article', undefined, 'glass collection-reader');
    panel.results.append(panel.list, panel.reader);
    panel.original = el('details', undefined, 'glass collection-original'); panel.original.hidden = true;
    panel.original.append(el('summary', 'Retained response and canonical evidence ledger'));
    panel.originalBody = el('div'); panel.original.append(panel.originalBody);
    host.append(panel.status, panel.sources, panel.counts, panel.results, panel.original,
      el('p', 'Partial coverage · 3 sources per tab · 30 items per source · 2 MiB per response · 4 retained versions per source · 30-second job deadline · no disk storage. No source truth, independence, event grouping or correction history is inferred. Network restrictions remain visible. Hidden text controls are displayed as Unicode escapes.', 'note'));
    const custom = el('details', undefined, 'collection-register glass');
    custom.append(el('summary', 'Add a public RSS / Atom source'));
    const form = el('form', undefined, 'news-filter');
    const name = el('input'); name.required = true; name.maxLength = 160;
    const endpoint = el('input'); endpoint.required = true; endpoint.type = 'url'; endpoint.maxLength = 2048; endpoint.placeholder = 'https://publisher.example/feed.xml';
    const submit = el('button', 'Add feed', 'btn'); submit.type = 'submit';
    form.append(labelled('Publisher / source name', name), labelled('Public HTTPS feed URL', endpoint), submit,
      el('p', 'Personal feed reading only. The publisher’s reuse rights remain unknown; this workflow keeps bounded session responses and supports no redistribution. Public HTTPS on port 443; redirects, local networks, credentials and linked-page crawling are excluded.', 'note'));
    form.onsubmit = async event => {
      event.preventDefault();
      const added = await execute(panel, 'collection_add_feed', { category, name: name.value, endpoint: endpoint.value });
      if (added) { form.reset(); custom.open = false; await refresh(category); }
    };
    custom.append(form); host.append(custom);
    panels.set(category, panel);
  }

  function install(panel, view) {
    panel.generation += 1;
    panel.view = view;
    const selectedSource = panel.source.value;
    panel.source.replaceChildren(el('option', 'All public sources')); panel.source.firstElementChild.value = '';
    for (const source of view.sources) {
      const option = el('option', source.source.name); option.value = source.source.id; panel.source.append(option);
    }
    panel.source.value = selectedSource;
    if (panel.source.selectedIndex < 0) panel.source.value = '';
    panel.sources.replaceChildren();
    for (const source of view.sources) {
      const card = el('details', undefined, 'glass collection-source');
      card.append(el('summary', source.source.name + ' · ' + source.outcome.replaceAll('_', ' ') + (source.stale ? ' · STALE retained response' : '')));
      card.append(el('p', source.message, 'note'));
      const rows = el('dl', undefined, 'kv');
      for (const [label, value] of [
        ['Endpoint', source.source.endpoint], ['Last attempt (UTC)', date(source.last_attempt_ms)],
        ['Last successful check (UTC)', date(source.last_success_ms)], ['Next request eligible (UTC)', source.next_attempt_ms ? date(source.next_attempt_ms) : 'Now'],
        ['Access', source.source.policy.access], ['Automated use', source.source.policy.automation],
        ['Retention', source.source.policy.retention], ['Redistribution', source.source.policy.redistribution], ['Policy reference', source.source.policy.reference]
      ]) rows.append(el('dt', label), el('dd', value));
      card.append(rows);
      for (const snapshot of source.snapshots) card.append(button('Inspect response · ' + date(snapshot.retrieved_at_ms) + ' · ' + snapshot.sha256.slice(0, 10), () => inspect(panel, source.source.id, snapshot.sha256, snapshot.request_url)));
      panel.sources.append(card);
    }
    panel.original.hidden = true; panel.originalBody.replaceChildren();
    renderEntries(panel);
  }

  function renderEntries(panel) {
    panel.list.replaceChildren();
    const query = panel.filter.value.toLocaleLowerCase();
    const all = panel.view?.sources.flatMap(source => source.entries.map(entry => ({ source, entry }))) ?? [];
    const matches = all.filter(({ source, entry }) => (!panel.source.value || panel.source.value === source.source.id)
      && (!query || [entry.title, entry.excerpt, source.source.name, entry.archive_reference].filter(Boolean).some(text => text.toLocaleLowerCase().includes(query))));
    panel.counts.textContent = matches.length + ' visible of ' + all.length + ' collected items. Source order and source item order; no global chronological sort or incident count.';
    for (const item of matches) {
      const { source, entry } = item;
      const row = el('button', undefined, 'news-row'); row.type = 'button';
      row.setAttribute('aria-pressed', String(panel.selected === entry.version_id));
      row.append(el('span', source.source.name + (source.stale ? ' / stale retained response' : ' / collected source material'), 'news-category'), el('strong', entry.title), el('span', entry.published_time ?? entry.archive_covering_dates ?? 'Publication time unreported'));
      row.onclick = () => { panel.selected = entry.version_id; renderEntries(panel); };
      const li = el('li'); li.append(row); panel.list.append(li);
    }
    const selected = matches.find(({ entry }) => entry.version_id === panel.selected) ?? matches[0];
    panel.reader.replaceChildren();
    if (!selected) { if (viewListener) viewListener(); panel.reader.append(el('h3', 'No matching retained items'), el('p', 'Inspect each source status above. Empty results do not establish absence outside the queried feeds or archive page.', 'note')); return; }
    panel.selected = selected.entry.version_id;
    const { source, entry } = selected;
    if (viewListener) viewListener();
    panel.reader.append(el('span', panel.category === 'news' ? 'Publisher feed excerpt / unreviewed' : 'Archive catalogue description / unreviewed', 'k'), el('h3', entry.title));
    panel.reader.append(el('pre', entry.excerpt ?? 'No excerpt supplied. Use the source reference to inspect the original.', 'collection-excerpt'));
    const rows = el('dl', undefined, 'kv');
    for (const [label, value] of [
      ['Source', source.source.name], ['Source URL', entry.url ?? 'No supported public HTTPS article URL supplied'],
      ['Provider identifier', entry.provider_id], ['Publication time (as supplied)', entry.published_time ?? 'Unreported'],
      ['Update time (as supplied)', entry.updated_time ?? 'Unreported'], ['Event time', 'Unknown; not inferred from publication or file dates'],
      ['Archive file covering dates', entry.archive_covering_dates ?? 'Not supplied'], ['Archive reference', entry.archive_reference ?? 'Not supplied'],
      ['Response retrieved (UTC)', date(source.current.retrieved_at_ms)], ['Response request', source.current.request_url],
      ['Record', entry.record_id], ['Extraction version', entry.version_id], ['Response SHA-256', entry.response_sha256],
      ['Item SHA-256', entry.item_sha256], ['Exact item bytes in response', entry.byte_start + '..' + entry.byte_end],
      ['Extraction', 'public-metadata-extraction-v1; XML decoding or JSON field extraction; source markup displayed as text'],
      ['Review / correction history', 'Unreviewed / unknown'], ['Source coverage', source.current.coverage_note]
    ]) rows.append(el('dt', label), el('dd', value));
    const provenance = el('details'); provenance.append(el('summary', 'Source, dates and provenance'), rows);
    panel.reader.append(provenance, button('Inspect retained original response', () => inspect(panel, source.source.id, entry.response_sha256, source.current.request_url)));
  }

  async function inspect(panel, sourceId, sha256, requestUrl) {
    const generation = panel.generation;
    try {
      const data = await request('collection_original', { sourceId, sha256, requestUrl });
      if (generation !== panel.generation) return;
      panel.originalBody.replaceChildren(el('p', data.source_id + ' / ' + data.request_url + ' / SHA-256 ' + data.sha256, 'note'));
      for (const [title, text] of [['Original response · UTF-8; controls escaped for display', data.text], ['Canonical evidence ledger · source material and linked extraction', data.evidence_ledger]]) {
        const detail = el('details'); const pre = el('pre', text); pre.tabIndex = 0;
        detail.append(el('summary', title), pre); panel.originalBody.append(detail);
      }
      panel.original.hidden = false; panel.original.open = true;
    } catch (error) { status(panel, error.message ?? String(error)); }
  }

  async function execute(panel, command, args) {
    if (running) { if (command === 'collection_refresh') panel.pendingRefresh = true; return false; }
    if (workspaceBusy?.()) { panel.pendingRefresh = command === 'collection_refresh'; status(panel, 'Waiting for the current workspace operation. Collection starts when it finishes.'); return false; }
    running = true;
    if (command === 'collection_refresh') { panel.started = true; panel.pendingRefresh = false; }
    for (const other of panels.values()) {
      other.refresh.disabled = true; other.cancel.disabled = other !== panel;
      if (other.clear) other.clear.disabled = true;
      other.host.querySelectorAll('form input, form button').forEach(input => { input.disabled = true; });
    }
    if (busyListener) busyListener(true, 'Collecting public sources · bounded network worker');
    status(panel, 'Collecting public sources. Prior responses remain until this operation succeeds. Source cadence and rate-limit backoff apply.');
    try {
      const view = await request(command, args); install(panel, view);
      const ready = view.sources.filter(source => source.outcome === 'ready' || source.outcome === 'empty').length;
      status(panel, ready + ' of ' + view.sources.length + ' sources have successful retained responses. Partial coverage; inspect source statuses and next eligible request times.');
      return true;
    } catch (error) {
      status(panel, (error.message ?? String(error)) + ' Earlier collected results remain unchanged.');
      return false;
    } finally {
      running = false;
      for (const other of panels.values()) {
        other.refresh.disabled = false; other.cancel.disabled = true;
        if (other.clear) other.clear.disabled = false;
        other.host.querySelectorAll('form input, form button').forEach(input => { input.disabled = false; });
      }
      if (busyListener) busyListener(false, 'Idle · public-source collection finished · session memory only');
    }
  }
  const refresh = category => execute(panels.get(category), 'collection_refresh', { category, query: panels.get(category).query });
  function schedule() {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    const panel = panels.get(active);
    if (panel?.auto.checked && !document.hidden) timer = setTimeout(async () => { await refresh(panel.category); schedule(); }, 15 * 60 * 1000);
  }
  function activate(instrument) {
    const previous = active;
    active = instrument === 'news' ? 'news' : instrument === 'witness' ? 'reports' : null;
    const panel = panels.get(active);
    if (panel && (!panel.started || (previous !== active && panel.auto.checked))) refresh(panel.category);
    else if (panel && previous !== active) updateFreshness(panel);
    schedule();
  }
  async function updateFreshness(panel) {
    const generation = panel.generation;
    try {
      const view = await request('collection_view', { category: panel.category });
      if (panel.generation === generation) install(panel, view);
    } catch (error) { status(panel, error.message ?? String(error)); }
  }
  mount('news'); mount('reports');
  for (const panel of panels.values()) {
    panel.clear = button('Clear retained public responses', () => execute(panel, 'collection_clear', { category: panel.category }));
    panel.host.append(panel.clear);
  }
  document.addEventListener('visibilitychange', () => {
    const panel = panels.get(active);
    if (panel && !document.hidden) {
      if (panel.auto.checked) refresh(panel.category); else updateFreshness(panel);
    }
    schedule();
  });
  return {
    activate,
    summary(category) {
      const panel = panels.get(category);
      if (!panel?.view) return null;
      const sources = panel.view.sources;
      const selected = sources.flatMap(source => source.entries.map(entry => ({source, entry}))).find(({entry}) => entry.version_id === panel.selected);
      return { loaded: sources.some(source => source.current), source: safe(selected?.source.source.name ?? 'Public sources'),
        hash: selected ? selected.entry.response_sha256.slice(0, 10) + '...' : '?',
        coverage: 'Partial ? ' + sources.filter(source => source.current).length + '/' + sources.length + ' sources' + (sources.some(source => source.stale) ? ' ? stale retained data' : ''),
        selection: selected?.entry.version_id ?? 'none' };
    },
    onView(callback) { viewListener = callback; },
    onBusy(callback, isBusy) { busyListener = callback; workspaceBusy = isBusy; },
    workspaceIdle() {
      const panel = panels.get(active);
      if (panel && (!panel.started || panel.pendingRefresh)) queueMicrotask(() => { if (panels.get(active) === panel && !running) refresh(panel.category); });
    }
  };
})();
