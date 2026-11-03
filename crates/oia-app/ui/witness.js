'use strict';
// Witness instrument presentation. Rust owns the account, identifiers and transitions.
// This page holds only uncommitted form input and the latest WitnessView response.
const Witness = (() => {
  const $ = id => document.getElementById(id);
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const SCHEMA = 'oia-local-tools/1';
  const FIELDS = [
    ['narrative', 'Narrative'],
    ['reported_time', 'Reported time'],
    ['location', 'Location'],
    ['duration', 'Duration'],
    ['direction', 'Direction'],
    ['apparent_size', 'Apparent size'],
    ['motion', 'Motion'],
    ['sound', 'Sound'],
    ['conditions', 'Conditions']
  ];
  let view = null;
  let busy = false;
  let active = false;
  let revisionDirty = false;
  let onViewListener = null;
  // Recreation prompt (MEDIA-04): Rust holds the last validated prompt; the page holds only
  // the uncommitted declarations (assumptions, unknowns, context choices) until Build.
  const CONTEXT = [
    ['scene', 'Scene', 'location'],
    ['time', 'Time', 'reported_time'],
    ['viewpoint', 'Viewpoint', null],
    ['lighting', 'Lighting', 'conditions'],
    ['scale', 'Apparent scale', 'apparent_size']
  ];
  let recreation = null;
  let recreationSaved = null;

  function node(tag, text, className) {
    const el = document.createElement(tag);
    if (text !== undefined) el.textContent = text;
    if (className) el.className = className;
    return el;
  }
  function showError(message) {
    $('error-message').textContent = message;
    $('error').hidden = false;
  }
  function status(message, partial = false) {
    const el = $('witness-status');
    el.textContent = message;
    el.className = partial ? 'coverage partial' : 'coverage';
  }
  function setBusy(value) {
    busy = value;
    $('view-witness').querySelectorAll('button, input, textarea, select').forEach(control => { control.disabled = value; });
  }
  function readForm(prefix) {
    const form = {};
    for (const [field] of FIELDS) {
      const value = $(`${prefix}-${field}`).value;
      form[field] = field === 'narrative' || value !== '' ? value : null;
    }
    return form;
  }
  function writeForm(prefix, form) {
    for (const [field] of FIELDS) $(`${prefix}-${field}`).value = form[field] ?? '';
  }
  function renderForm(host, form) {
    host.replaceChildren();
    for (const [field, label] of FIELDS) {
      const value = form[field];
      host.append(
        node('dt', label),
        node('dd', value === null ? '— unreported' : value, value === null ? 'none witness-text' : 'witness-text')
      );
    }
  }
  function renderVersion(version, initial = false) {
    const article = node('article', undefined, 'witness-version glass');
    const heading = node('div', undefined, 'witness-version-head');
    heading.append(
      node('span', initial ? 'Initial account' : `Revision · ${version.version}`, 'k'),
      node('span', version.after_explanations ? 'After in-workflow explanations' : 'Before in-workflow explanations', version.after_explanations ? 'chip warn' : 'chip')
    );
    article.append(heading);
    const details = node('dl', undefined, 'kv witness-fields');
    renderForm(details, version.form);
    article.append(details);
    const rationale = version.rationale === null ? 'Not applicable to the initial account.' : version.rationale;
    article.append(node('p', `Rationale: ${rationale}`, 'note witness-text'));
    return article;
  }
  function renderInspector(capture) {
    const rows = [
      ['Storage', 'Memory · discarded on exit'],
      ['Capture ID', capture?.id ?? null],
      ['Origin', capture ? (capture.origin === 'reported' ? 'Reported testimony' : 'Simulated account') : null],
      ['Latest version', capture?.latest.version ?? null],
      ['Revisions', capture ? String(capture.revisions.length) : null],
      ['First exposure', capture?.first_exposure_version ?? null],
      ['Actor', capture?.latest.actor ?? null]
    ];
    const dl = $('witness-record');
    dl.replaceChildren();
    for (const [key, value] of rows) {
      dl.append(node('dt', key), node('dd', value ?? '— not recorded', value === null ? 'none' : ''));
    }
    $('insp-witness-title').textContent = capture ? `${capture.origin === 'reported' ? 'Reported' : 'Simulated'} account` : 'No account captured';
    $('insp-witness-origin').textContent = capture ? (capture.origin === 'reported' ? '▣ Reported' : '▣ Simulated') : '▣ No account';
    $('insp-witness-exposure').textContent = capture?.first_exposure_version ? 'Explanations exposed' : 'Explanations not exposed here';
    $('witness-exposure-qualification').textContent = view?.exposure_qualification ?? '';
    $('witness-limits').textContent = view
      ? `Each field and rationale is limited to ${view.max_field_bytes.toLocaleString('en-US')} UTF-8 bytes; at most ${view.max_versions} versions including the initial account. Values remain exact reported text, not normalized time, coordinates or measurements. No durable save, common evidence envelope, capture timestamp, authorization, sky sketch or regional/event link is implemented.`
      : 'Witness limits have not been loaded.';
  }
  // ---- Recreation prompt: declarations are page input; the prompt itself is Rust's ----
  function versionsOf(capture) {
    return [capture.initial, ...capture.revisions];
  }
  function selectedAccount(capture) {
    const wanted = $('recreation-version').value;
    return versionsOf(capture).find(v => v.version === wanted) ?? capture.latest;
  }
  let rowSequence = 0;
  function declarationRows(hostId) {
    return [...$(hostId).querySelectorAll('.rec-row')].map(row => [...row.querySelectorAll('input')].map(input => input.value));
  }
  // Context choices are bound to a row's stable id, so removing an earlier assumption never
  // silently re-binds a field to a different one; the index is computed only when sending.
  function assumptionIds() {
    return [...$('recreation-assumptions').querySelectorAll('.rec-row')].map(row => row.dataset.rowId);
  }
  function addDeclaration(hostId, placeholders) {
    const row = node('div', undefined, 'rec-row');
    row.dataset.rowId = String(++rowSequence);
    for (const placeholder of placeholders) {
      const input = node('input');
      input.placeholder = placeholder; input.maxLength = 2048; input.required = true;
      input.setAttribute('aria-label', placeholder);
      input.oninput = () => { if (hostId === 'recreation-assumptions') renderContextChoices(); };
      row.append(input);
    }
    const remove = node('button', 'Remove', 'btn');
    remove.type = 'button';
    remove.onclick = () => { row.remove(); renderContextChoices(); };
    row.append(remove);
    $(hostId).append(row);
    row.querySelector('input').focus();
  }
  function renderVersionOptions(capture) {
    const select = $('recreation-version');
    const current = select.value;
    select.replaceChildren();
    for (const account of versionsOf(capture)) {
      const option = node('option', account === capture.initial ? `${account.version} · initial account` : `${account.version} · revision`);
      option.value = account.version;
      select.append(option);
    }
    select.value = versionsOf(capture).some(v => v.version === current) ? current : capture.latest.version;
  }
  function renderContextChoices() {
    const capture = view?.capture;
    const host = $('recreation-context');
    host.replaceChildren();
    if (!capture) return;
    const account = selectedAccount(capture);
    const assumptions = declarationRows('recreation-assumptions').map(([assumption]) => assumption);
    const ids = assumptionIds();
    for (const [key, label, field] of CONTEXT) {
      const reported = field ? account.form[field] : null;
      const row = node('div', undefined, 'rec-ctx');
      row.append(node('span', label, 'k'));
      if (reported !== null) {
        row.append(node('span', `Reported in ${account.version}: ${reported}`, 'rec-ctx-reported witness-text'));
      } else if (key === 'scale') {
        row.append(node('span', `Unknown · not reported in ${account.version}; no size or range is invented`, 'rec-ctx-unknown'));
      } else {
        const select = node('select');
        select.id = `recreation-ctx-${key}`;
        select.setAttribute('aria-label', `${label}: unknown or a declared assumption`);
        const previous = host.dataset[key] ?? '';
        const unknown = node('option', `Unknown · not reported in ${account.version}`);
        unknown.value = '';
        select.append(unknown);
        assumptions.forEach((assumption, i) => {
          const option = node('option', `Assumption ${i + 1}: ${assumption || '(empty)'}`);
          option.value = ids[i];
          select.append(option);
        });
        select.value = [...select.options].some(o => o.value === previous) ? previous : '';
        select.onchange = () => { host.dataset[key] = select.value; };
        row.append(select);
      }
      host.append(row);
    }
  }
  function readRecreationRequest(capture) {
    const context = {};
    const ids = assumptionIds();
    for (const [key] of CONTEXT) {
      if (key === 'scale') continue;
      const select = $(`recreation-ctx-${key}`);
      const index = select && select.value !== '' ? ids.indexOf(select.value) : -1;
      context[key] = index >= 0 ? index : null;
    }
    return {
      version: selectedAccount(capture).version,
      title: $('recreation-title').value,
      assumptions: declarationRows('recreation-assumptions').map(([assumption, rationale]) => ({ assumption, rationale })),
      unknowns: declarationRows('recreation-unknowns').map(([aspect, impact]) => ({ aspect, impact })),
      context
    };
  }
  function renderRecreation() {
    const prompt = recreation?.prompt ?? null;
    $('recreation-result').hidden = prompt === null;
    const record = $('recreation-record');
    record.replaceChildren();
    const rows = [
      ['Prompt ID', prompt?.prompt_id ?? null],
      ['Version', prompt?.prompt_version ?? null],
      ['Case', prompt ? `${prompt.case_id} · session-local, no case record` : null],
      ['Source', prompt ? `${prompt.source_version} · ${prompt.source_origin}${prompt.source_after_explanations ? ' · recorded after candidate explanations were exposed' : ''}` : null],
      ['Rationale', prompt ? (prompt.source_rationale ?? '— initial account, no revision rationale') : null],
      ['Bytes', prompt ? prompt.bytes.toLocaleString('en-US') : null],
      ['SHA-256', prompt?.sha256 ?? null],
      ['Saved', recreationSaved && recreationSaved.prompt_version === prompt?.prompt_version ? recreationSaved.path : null]
    ];
    for (const [key, value] of rows) record.append(node('dt', key), node('dd', value ?? '— none', value === null ? 'none' : ''));
    $('recreation-limits').textContent = recreation
      ? `${recreation.retention} Limits: ${recreation.limits.max_reported_details} reported details (narrative lines and fields), ${recreation.limits.max_assumptions} assumptions, ${recreation.limits.max_unknowns} unknowns, ${recreation.limits.max_text_bytes.toLocaleString('en-US')} text bytes, ${recreation.limits.max_output_bytes.toLocaleString('en-US')} output bytes. Single witness only: conflicting variants, reference assets and imported-recreation checks are not in this build.`
      : 'Recreation limits have not been loaded.';
    if (!prompt) return;
    $('recreation-prompt-version').textContent = prompt.prompt_version;
    $('recreation-source-version').textContent = `from ${prompt.source_version} · ${prompt.source_origin}${prompt.source_after_explanations ? ' · after explanations' : ''}`;
    $('recreation-source-version').className = prompt.source_origin === 'simulated account' || prompt.source_after_explanations ? 'chip warn' : 'chip';
    $('recreation-label').textContent = recreation.visible_label;
    $('recreation-caveat').textContent = recreation.caveat;
    const context = $('recreation-context-rows');
    context.replaceChildren();
    const status = { reported: ['Reported', ''], assumed: ['Assumed', 'est'], unknown: ['Unknown', 'none'] };
    for (const row of prompt.context) {
      const [word, cls] = status[row.status] ?? [row.status, ''];
      context.append(node('dt', row.field), node('dd', `${word} · ${row.text}`, `${cls} witness-text`.trim()));
    }
    $('recreation-text').value = prompt.text;
    $('recreation-counts').textContent = `${prompt.reported_details} reported details · ${prompt.assumptions} assumptions · ${prompt.unknowns} unknowns · ${prompt.bytes.toLocaleString('en-US')} bytes · exact text; existing files are never overwritten`;
    const saved = $('recreation-saved');
    const current = recreationSaved && recreationSaved.prompt_version === prompt.prompt_version;
    saved.hidden = !current;
    if (current) {
      saved.replaceChildren(node('span', 'Prompt text saved', 'k'), node('div', recreationSaved.path), node('div', `${recreationSaved.bytes.toLocaleString('en-US')} bytes · labeled illustration brief · witness account unchanged`), node('div', `SHA-256 ${recreationSaved.sha256}`));
    }
  }
  async function callRecreation(command, args, success) {
    if (busy) return false;
    $('error').hidden = true;
    setBusy(true);
    try {
      const response = await invoke(command, args);
      if (response.schema !== SCHEMA) throw { message: 'This application response uses an unsupported version.' };
      recreation = response.data;
      renderRecreation();
      if (success) status(success);
      return true;
    } catch (error) {
      showError(error?.message || String(error));
      status('Recreation prompt refused; the previous prompt, if any, is unchanged.', true);
      return false;
    } finally {
      setBusy(false);
    }
  }

  function render() {
    const capture = view?.capture ?? null;
    $('witness-capture-panel').hidden = capture !== null;
    $('witness-account').hidden = capture === null;
    $('witness-retention').textContent = view?.retention ?? 'Memory only; discarded on exit.';
    if (capture) {
      renderForm($('witness-initial-fields'), capture.initial.form);
      $('witness-initial-version').textContent = capture.initial.version;
      $('witness-account-origin').textContent = capture.origin === 'reported' ? 'Reported testimony' : 'Simulated account';
      $('witness-account-origin').className = capture.origin === 'reported' ? 'chip' : 'chip warn';
      const exposed = capture.first_exposure_version !== null;
      $('witness-expose').hidden = exposed;
      $('witness-explanations').hidden = !exposed;
      $('witness-exposure-state').textContent = exposed
        ? `Unassessed prompts exposed at ${capture.first_exposure_version}. They are not generated findings, rankings or evidence-backed conclusions.`
        : 'Candidate prompts remain hidden until you explicitly expose them. The initial account is retained above.';
      const revisions = $('witness-revisions');
      revisions.replaceChildren();
      for (const revision of capture.revisions) revisions.append(renderVersion(revision));
      $('witness-no-revisions').hidden = capture.revisions.length !== 0;
      if (!revisionDirty) {
        writeForm('witness-revision', capture.latest.form);
        $('witness-rationale').value = '';
      }
      renderVersionOptions(capture);
      renderContextChoices();
    } else {
      $('witness-explanations').hidden = true;
      $('witness-revisions').replaceChildren();
    }
    renderInspector(capture);
    renderRecreation();
    if (onViewListener) onViewListener(summary());
  }
  function applyView(next) {
    view = next;
    render();
  }
  async function call(command, args, success) {
    if (busy) return false;
    $('error').hidden = true;
    setBusy(true);
    try {
      const response = await invoke(command, args);
      if (response.schema !== SCHEMA) throw { message: 'This application response uses an unsupported version.' };
      applyView(response.data);
      if (success) status(success);
      return true;
    } catch (error) {
      showError(error?.message || String(error));
      status('Witness command failed; inspect the current account before retrying.', true);
      return false;
    } finally {
      setBusy(false);
    }
  }
  function summary() {
    const capture = view?.capture ?? null;
    return {
      source: capture ? `${capture.origin === 'reported' ? 'Reported' : 'Simulated'} witness` : 'No witness account',
      coverage: capture ? `${capture.revisions.length} revisions · memory only` : 'memory only',
      selection: capture?.latest.version ?? 'none',
      loaded: capture !== null,
      partial: true
    };
  }
  async function activate() {
    active = true;
    if (!view) await call('witness_view', {}, '');
    if (!recreation) await callRecreation('recreation_view', {}, '');
  }
  function deactivate() { active = false; }
  function wire() {
    $('witness-capture-form').onsubmit = async event => {
      event.preventDefault();
      revisionDirty = false;
      const captured = await call('witness_capture', {
        origin: $('witness-origin').value,
        form: readForm('witness-initial')
      }, 'Initial account captured in memory · original retained');
      if (captured) $('witness-capture-form').reset();
    };
    $('witness-revision-form').oninput = () => { revisionDirty = true; };
    $('witness-revision-form').onsubmit = async event => {
      event.preventDefault();
      const capture = view?.capture;
      if (!capture) return;
      const revised = await call('witness_revise', {
        expectedVersion: capture.latest.version,
        form: readForm('witness-revision'),
        rationale: $('witness-rationale').value
      }, 'Revision appended · initial account unchanged');
      if (revised) {
        revisionDirty = false;
        writeForm('witness-revision', view.capture.latest.form);
        $('witness-rationale').value = '';
      }
    };
    $('witness-expose').onclick = async () => {
      const capture = view?.capture;
      if (!capture) return;
      await call('witness_expose_explanations', { expectedVersion: capture.latest.version },
        'Unassessed explanation prompts exposed · workflow marker retained');
    };
    $('recreation-version').onchange = renderContextChoices;
    $('recreation-add-assumption').onclick = () => addDeclaration('recreation-assumptions', ['Assumption, for framing only', 'Rationale']);
    $('recreation-add-unknown').onclick = () => addDeclaration('recreation-unknowns', ['Unknown aspect', 'Impact on the recreation']);
    $('recreation-form').onsubmit = async event => {
      event.preventDefault();
      const capture = view?.capture;
      if (!capture) return;
      await callRecreation('recreation_export', { request: readRecreationRequest(capture) },
        'Recreation prompt built · labeled illustration brief · nothing saved');
    };
    $('recreation-save').onclick = () => {
      const prompt = recreation?.prompt;
      if (!prompt || busy) return;
      // The shared single-operation worker (app.js) owns the dialog, staging and cancellation.
      operation('recreation_save', { promptVersion: prompt.prompt_version }, 'Choose a new filename · writing the prompt text follows', data => {
        recreationSaved = data;
        renderRecreation();
        return `Saved recreation prompt · ${data.bytes.toLocaleString('en-US')} bytes · labeled illustration brief, not evidence`;
      });
    };
  }
  wire();
  return {
    activate,
    deactivate,
    summary,
    refresh: () => call('witness_view', {}, ''),
    onView: fn => { onViewListener = fn; },
    isActive: () => active
  };
})();
