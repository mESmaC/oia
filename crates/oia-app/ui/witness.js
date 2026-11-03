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
    } else {
      $('witness-explanations').hidden = true;
      $('witness-revisions').replaceChildren();
    }
    renderInspector(capture);
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
