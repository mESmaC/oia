'use strict';

const News = (() => {
  const element = id => document.getElementById(id);
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const kinds = {
    plan: 'Plan', reported_observation: 'Reported observation', allegation: 'Allegation',
    arrest: 'Arrest', charge: 'Charge', judgment: 'Judgment', attributed_statement: 'Attributed statement'
  };
  const states = {
    not_retracted: 'Not retracted', corrected_by_later_version: 'Corrected by later version',
    correction_issued: 'Correction issued', retracted: 'Retracted'
  };
  const publication = article => states[article.retraction_status] + ' / availability: ' + article.availability;
  let view = null;
  let selectedId = null;
  let busy = false;
  let listener = null;

  function safeText(value) {
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

  function node(tag, text, className) {
    const result = document.createElement(tag);
    if (text !== undefined) result.textContent = safeText(text);
    if (className) result.className = className;
    return result;
  }

  function status(message) { element('news-status').textContent = safeText(message); }

  function ready() {
    for (const id of ['news-open', 'news-example']) element(id).disabled = busy;
    for (const id of ['news-query', 'news-region', 'news-kind', 'news-apply', 'news-reset', 'news-clear']) {
      element(id).disabled = busy || !view;
    }
    element('news-cancel').disabled = !busy;
  }

  function details(id, rows) {
    element(id).replaceChildren();
    for (const [key, value] of rows) {
      element(id).append(node('dt', key), node('dd', value ?? 'Unreported', value === null ? 'none' : ''));
    }
  }

  function selected() { return view?.articles.find(article => article.version_id === selectedId) ?? null; }

  function renderSelection() {
    const article = selected();
    element('news-reader-title').textContent = article ? safeText(article.title) : 'No article version selected';
    element('news-reader-meta').textContent = article
      ? safeText(article.publisher + ' / ' + article.version_id + ' / ' + kinds[article.kind] + ' / declared ' + publication(article))
      : 'A zero-match filter says nothing about material outside this dossier.';
    element('news-text').textContent = article
      ? (article.text === null ? 'Text not retained. No inference about retraction or the truth of this account follows.' : safeText(article.text))
      : 'Select a version from the list, or reset the filters.';
    element('news-list').querySelectorAll('button').forEach(button => {
      button.setAttribute('aria-pressed', String(button.dataset.version === selectedId));
    });
    details('news-record', article ? [
      ['Record ID', article.record_id], ['Version ID', article.version_id],
      ['Source ID', article.source_id], ['Publisher', article.publisher],
      ['Declared region', article.region], ['Declared origin', article.origin_id], ['Proposed event', article.event_id],
      ['Category', kinds[article.kind]], ['Declared retraction status', states[article.retraction_status]],
      ['Declared availability', article.availability],
      ['Predecessor version', article.predecessor_version_id], ['Superseded by version', article.superseded_by_version_id],
      ['Declared lineage (not independent support)', article.lineage.length
        ? article.lineage.map(edge => edge.relation_kind + ' -> ' + edge.to_version_id + ' [' + edge.edge_id + ']').join('; ') : null],
      ['Publication time (declared)', article.published_time], ['Update time (declared)', article.updated_time],
      ['Event time (declared)', article.event_time], ['Reference (not fetched)', article.url],
      ['Text SHA-256', article.text_sha256]
    ] : [['Selection', null]]);
    if (listener) listener();
  }

  function render() {
    element('news-loaded').hidden = !view;
    element('news-origin').textContent = view ? (view.synthetic ? 'Declared synthetic / not observed evidence' : 'Imported / unverified declarations') : 'No dossier loaded';
    element('news-title').textContent = view ? safeText(view.title) : 'Inspect the reporting, not just the headline.';
    element('news-counts').replaceChildren();
    element('news-list').replaceChildren();
    element('news-sources').replaceChildren();
    if (view) {
      for (const [count, label] of [
        [view.visible.versions, 'Visible versions'], [view.visible.records, 'Article records'],
        [view.visible.sources, 'Declared source IDs'], [view.visible.missing_text_versions, 'Text not retained']
      ]) {
        const card = node('div', undefined, 'news-stat');
        card.append(node('b', count), node('span', label));
        element('news-counts').append(card);
      }
      element('news-coverage').textContent = 'Collection coverage declared ' + view.collection_coverage + ': ' + safeText(view.coverage_note);
      element('news-results-note').textContent = view.visible.versions + ' of ' + view.total.versions + ' retained versions. ' +
        view.visible.proposed_events + ' proposed event groups; ' + view.visible.ungrouped_versions + ' ungrouped versions; ' +
        view.visible.unknown_origin_versions + ' with unknown origin; ' + view.visible.missing_text_versions + ' without retained text. Counts describe the visible subset only; origin/independence totals are not assessed.';
      for (const article of view.articles) {
        const row = node('button', undefined, 'news-row');
        row.type = 'button';
        row.dataset.version = article.version_id;
        row.setAttribute('aria-pressed', String(article.version_id === selectedId));
        row.append(node('span', kinds[article.kind] + ' / declared ' + publication(article), 'news-category'));
        row.append(node('strong', article.title), node('span', article.publisher + ' / ' + article.version_id));
        row.append(node('span', 'Origin: ' + (article.origin_id ?? 'unknown') + ' / Event: ' + (article.event_id ?? 'ungrouped')));
        row.onclick = () => { selectedId = article.version_id; renderSelection(); };
        const item = node('li');
        item.append(row);
        element('news-list').append(item);
      }
      for (const source of view.source_roster) {
        element('news-sources').append(node('li', source.publisher + ' [' + source.source_id + '] / ' + source.counts.versions + ' versions / ' + source.counts.records + ' records'));
      }
      details('news-snapshot', [
        ['Schema', 'oia-news-dossier/1'], ['Original bytes', String(view.original_bytes)],
        ['Dossier SHA-256', view.sha256], ['Retained versions', String(view.total.versions)],
        ['Collection coverage (declared)', view.collection_coverage],
        ['Provenance', view.synthetic ? 'Declared synthetic' : 'Imported, unverified'], ['Storage', 'Session memory only']
      ]);
    } else {
      element('news-coverage').textContent = '';
      details('news-snapshot', [['Dossier', null]]);
    }
    renderSelection();
    ready();
  }

  function install(data, reset) {
    view = data;
    if (reset) {
      element('news-filter').reset();
      element('news-region').replaceChildren(node('option', 'All regions, including unreported'));
      element('news-region').firstElementChild.value = '';
      for (const region of view.regions) {
        const option = node('option', region);
        option.value = region;
        element('news-region').append(option);
      }
      selectedId = null;
    }
    if (!selected()) selectedId = view.articles[0]?.version_id ?? null;
    element('news-pending').hidden = true;
    render();
    status(view.visible.versions ? 'Showing the selected imported versions. No live collection or independent verification.' : 'No matching article versions in this dossier. Collection coverage remains ' + view.collection_coverage + '.');
  }

  async function execute(command, args, accept) {
    if (busy) return;
    busy = true;
    ready();
    status('Processing locally. Previous results remain until this operation succeeds.');
    try {
      const reply = await invoke(command, args);
      if (reply?.schema !== 'oia-local-tools/1') throw new Error('Unsupported News response schema.');
      accept(reply.data);
    } catch (error) {
      status((error.message ?? String(error)) + ' Previous dossier and displayed results are unchanged.');
    } finally {
      busy = false;
      ready();
    }
  }

  function applyFilters() {
    if (!view) return;
    execute('news_query', { sha256: view.sha256, filter: {
      query: element('news-query').value,
      region: element('news-region').value || null,
      kind: element('news-kind').value || null
    } }, data => install(data, false));
  }

  element('news-open').onclick = () => execute('open_news', {}, data => {
    if (data === null) status('Import dialog cancelled. Previous dossier and filters retained.');
    else install(data, true);
  });
  element('news-example').onclick = () => execute('news_example', {}, data => install(data, true));
  element('news-clear').onclick = () => execute('news_clear', {}, () => {
    view = null;
    selectedId = null;
    element('news-filter').reset();
    element('news-pending').hidden = true;
    render();
    status('Dossier unloaded. Import a local JSON file or explicitly load the synthetic example.');
  });
  element('news-cancel').onclick = async () => {
    try {
      await invoke('cancel_operation');
      if (busy) status('Cancellation requested. Previous dossier remains until the operation completes.');
    } catch (error) {
      if (busy) status(error.message ?? String(error));
    }
  };
  element('news-filter').onsubmit = event => { event.preventDefault(); applyFilters(); };
  element('news-filter').oninput = () => { if (view) element('news-pending').hidden = false; };
  element('news-reset').onclick = () => { element('news-filter').reset(); applyFilters(); };
  render();
  return {
    onView(callback) { listener = callback; },
    summary() {
      return {
        loaded: !!view, synthetic: view?.synthetic === true,
        source: view ? safeText(view.title) : 'No news dossier',
        hash: view ? view.sha256.slice(0, 8) + '...' : '—',
        coverage: view ? 'declared ' + view.collection_coverage : 'Not collected',
        selection: selectedId ?? 'none'
      };
    }
  };
})();
