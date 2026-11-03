'use strict';
// Globe instrument presentation (docs/implementation/globe-instrument.md). Rust owns the case,
// the imagery registry, the camera and the selection through the `globe_*` commands. This file
// loads the pinned CesiumJS build from `vendor/cesium/`, draws the GlobeView it is sent, and
// sends intents. Local state is presentation only: Cesium objects, projected positions, tile
// counters the renderer observed, and the overlays toggle. Nothing here computes a domain value.
const Globe = (() => {
  const $ = id => document.getElementById(id);
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const SCHEMA = 'oia-local-tools/1';
  const CESIUM_BASE = 'vendor/cesium/';
  const CAMERA_DEBOUNCE_MS = 200;
  const WORLD_HEIGHT_M = 20000000;

  let view = null;            // last GlobeView from Rust
  let viewer = null;          // Cesium.Viewer
  let rendererState = 'idle'; // idle | loading | ready | unavailable
  let overlays = true;
  let reduced = false;
  let active = false;
  let plottedKey = '';
  let cameraTimer = null;
  let suppressCameraReport = false;
  const tiles = { loaded: 0, failed: 0, cancelled: 0, pending: 0 };
  const FLIGHT_MS = 320; // visual language v2 §5.5 coordinated selection; immediate under reduced motion
  const prefersReducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const LOG_LIMIT = 24;
  const log = [];               // renderer messages the page observed, newest last, bounded
  function note(kind, text) {
    log.push(`${kind}: ${String(text).replace(/\s+/g, ' ').slice(0, 300)}`);
    if (log.length > LOG_LIMIT) log.splice(0, log.length - LOG_LIMIT);
    const host = document.getElementById('globe-log');
    if (host) host.replaceChildren(...log.map(line => { const li = document.createElement('li'); li.textContent = line; return li; }));
  }
  window.addEventListener('error', event => note('error', `${event.message} (${(event.filename || '').split('/').pop()}:${event.lineno})`));
  window.addEventListener('unhandledrejection', event => note('rejection', event.reason?.message || event.reason));
  document.addEventListener('securitypolicyviolation', event => note('csp', `${event.violatedDirective} blocked ${event.blockedURI || 'inline'}`));
  const drawn = new Map();    // record id -> { anchor, points, kind, cls }
  const tagEls = new Map();   // record id -> button
  let onViewListener = null;

  const fmt = n => Number(n).toLocaleString('en-US');
  const pad2 = n => String(n).padStart(2, '0');
  const deg = (v, digits = 4) => Number(v).toFixed(digits);
  const node = (tag, text, className) => {
    const el = document.createElement(tag);
    if (text !== undefined) el.textContent = text;
    if (className) el.className = className;
    return el;
  };
  const kv = (dl, rows) => {
    dl.replaceChildren();
    for (const [key, value, cls] of rows) {
      dl.append(node('dt', key), node('dd', value === null || value === undefined || value === '' ? '— not recorded' : String(value), value === null || value === undefined || value === '' ? 'none' : (cls || '')));
    }
  };
  const showError = message => {
    $('error-message').textContent = message;
    $('error').hidden = false;
  };
  async function call(command, args) {
    try {
      const response = await invoke(command, args);
      if (response.schema !== SCHEMA) throw { message: 'This application response uses an unsupported version.' };
      applyView(response.data);
      return true;
    } catch (error) {
      showError(error?.message || String(error));
      return false;
    }
  }

  // ---- Renderer lifecycle ----
  function unavailable(title, text) {
    rendererState = 'unavailable';
    $('globe-unavailable').hidden = false;
    $('globe-unavail-title').textContent = title;
    $('globe-unavail-text').textContent = text;
    renderState(); renderAttribution(); notify();
  }
  function loadCesium() {
    if (rendererState !== 'idle') return;
    rendererState = 'loading';
    $('globe-unavailable').hidden = false;
    $('globe-unavail-title').textContent = 'Loading the globe renderer';
    $('globe-unavail-text').textContent = `CesiumJS ${view?.renderer.pinned_version || ''} from the bundled vendor directory. Imagery tiles are requested from the registered source only after the renderer is ready.`;
    window.CESIUM_BASE_URL = CESIUM_BASE;
    // Record why the bundle or the viewer failed, so the empty state can say so instead of guessing.
    const problems = [];
    const onError = event => { problems.push(`${String(event.message || 'script error').slice(0, 240)}${event.filename ? ` (${event.filename.split('/').pop()}:${event.lineno})` : ''}`); };
    const onViolation = event => { problems.push(`CSP blocked ${event.violatedDirective} for ${event.blockedURI || 'inline'}`); };
    window.addEventListener('error', onError);
    document.addEventListener('securitypolicyviolation', onViolation);
    const detach = () => { window.removeEventListener('error', onError); document.removeEventListener('securitypolicyviolation', onViolation); };
    const script = document.createElement('script');
    script.src = `${CESIUM_BASE}Cesium.js`;
    script.onload = () => {
      try { createViewer(); }
      catch (error) { problems.push(String(error?.stack || error?.message || error).split('\n').slice(0, 3).join(' ').slice(0, 400)); unavailable('The globe renderer failed to start', `${problems.join(' · ')}. WebGL may be unavailable in this WebView.`); }
      finally { detach(); }
    };
    script.onerror = () => { detach(); unavailable('CesiumJS build not present', 'The pinned CesiumJS build is not in crates/oia-app/ui/vendor/cesium. Run scripts/run fetch (or scripts/fetch-ui-vendor), rebuild, and relaunch. Nothing else in the workbench depends on it.'); };
    document.head.append(script);
  }
  function notify() { if (onViewListener) onViewListener(summary()); }
  function createViewer() {
    const Cesium = window.Cesium;
    if (!Cesium) throw new Error('window.Cesium is undefined after the script loaded');
    Cesium.Ion.defaultAccessToken = '';
    // The combined build also carries its worker code as a string (CESIUM_WORKERS) and, when it
    // is present, starts workers from blob: URLs with importScripts, which the CSP refuses.
    // Without it Cesium loads the same-origin module workers from vendor/cesium/Workers/.
    delete globalThis.CESIUM_WORKERS;
    // GIBS asks for no bulk use: at most six tile requests in flight to it (Cesium's default is 18).
    Cesium.RequestScheduler.requestsByServer['gibs.earthdata.nasa.gov:443'] = 6;
    viewer = new Cesium.Viewer($('globe-canvas'), {
      animation: false, baseLayerPicker: false, fullscreenButton: false, geocoder: false, homeButton: false,
      infoBox: false, sceneModePicker: false, selectionIndicator: false, timeline: false,
      navigationHelpButton: false, navigationInstructionsInitiallyVisible: false, vrButton: false,
      baseLayer: false, skyBox: false, shouldAnimate: false,
      requestRenderMode: true, maximumRenderTimeChange: Infinity,
      creditContainer: $('globe-credit-sink')
    });
    const scene = viewer.scene;
    scene.backgroundColor = Cesium.Color.BLACK;
    scene.globe.baseColor = Cesium.Color.fromCssColorString('#0b0d10'); // missing tiles show as dark surface, not sea
    scene.globe.enableLighting = false; // a live sun would be a live cue on a historical case
    scene.globe.showGroundAtmosphere = !reduced;
    scene.skyAtmosphere.show = !reduced;
    scene.fog.enabled = !reduced;
    if (scene.sun) scene.sun.show = false;   // not created when skyBox is false
    if (scene.moon) scene.moon.show = false;
    scene.highDynamicRange = false;
    const controller = scene.screenSpaceCameraController;
    controller.minimumZoomDistance = 50;
    controller.maximumZoomDistance = 60000000;
    scene.globe.tileLoadProgressEvent.addEventListener(pending => { tiles.pending = pending; renderAttribution(); });
    scene.postRender.addEventListener(positionOverlays);
    viewer.camera.moveEnd.addEventListener(onCameraMoveEnd);
    const handler = new Cesium.ScreenSpaceEventHandler(scene.canvas);
    handler.setInputAction(onClick, Cesium.ScreenSpaceEventType.LEFT_CLICK);
    rendererState = 'ready';
    $('globe-unavailable').hidden = true;
    if (view) {
      setCamera(view.camera, false);
      applyView(view);
    }
    notify();
  }

  // ---- Camera ----
  function setCamera(camera, fly) {
    if (!viewer) return;
    const Cesium = window.Cesium;
    suppressCameraReport = true;
    const target = {
      destination: Cesium.Cartesian3.fromDegrees(camera.lon_deg, camera.lat_deg, camera.height_m),
      orientation: { heading: Cesium.Math.toRadians(camera.heading_deg), pitch: Cesium.Math.toRadians(camera.pitch_deg), roll: 0 }
    };
    viewer.camera.cancelFlight(); // including the viewer's own initial flight to its default view
    const release = () => { suppressCameraReport = false; reportCamera(); };
    if (fly && !reduced && !prefersReducedMotion()) viewer.camera.flyTo({ ...target, duration: FLIGHT_MS / 1000, complete: release, cancel: release });
    else { viewer.camera.flyTo({ ...target, duration: 0 }); suppressCameraReport = false; }
    viewer.scene.requestRender();
  }
  function currentCamera() {
    const Cesium = window.Cesium;
    const c = viewer.camera.positionCartographic;
    let heading = Cesium.Math.toDegrees(viewer.camera.heading) % 360;
    if (heading < 0) heading += 360;
    let lon = Cesium.Math.toDegrees(c.longitude);
    if (lon > 180) lon -= 360; if (lon < -180) lon += 360;
    return {
      lat_deg: Math.max(-90, Math.min(90, Cesium.Math.toDegrees(c.latitude))),
      lon_deg: Math.max(-180, Math.min(180, lon)),
      height_m: Math.max(50, Math.min(60000000, c.height)),
      heading_deg: Math.min(360, heading),
      pitch_deg: Math.max(-90, Math.min(90, Cesium.Math.toDegrees(viewer.camera.pitch)))
    };
  }
  function onCameraMoveEnd() {
    if (suppressCameraReport) return;
    clearTimeout(cameraTimer);
    cameraTimer = setTimeout(reportCamera, CAMERA_DEBOUNCE_MS);
  }
  function reportCamera() {
    if (!viewer || !view) return;
    const camera = currentCamera();
    if (Object.values(camera).some(v => !Number.isFinite(v))) return;
    call('globe_camera', { camera });
  }
  function pan(dx, dy) {
    if (!viewer) return;
    const step = viewer.camera.positionCartographic.height * 0.12;
    if (dx) viewer.camera.moveRight(dx * step);
    if (dy) viewer.camera.moveUp(dy * step);
    viewer.scene.requestRender();
  }
  function zoom(direction) {
    if (!viewer) return;
    const amount = viewer.camera.positionCartographic.height * 0.3;
    if (direction > 0) viewer.camera.zoomIn(amount); else viewer.camera.zoomOut(amount);
    viewer.scene.requestRender();
  }
  function home() { if (view) setCamera(view.home, true); }
  function world() {
    if (!view) return;
    setCamera({ lat_deg: view.home.lat_deg, lon_deg: view.home.lon_deg, height_m: WORLD_HEIGHT_M, heading_deg: 0, pitch_deg: -90 }, true);
  }

  // ---- Imagery ----
  let baseKey = '', overlayKey = '', baseLayer = null, overlayLayer = null;
  function setImagery(imagery) {
    if (!viewer) return;
    const nextBase = imagery.active.url;
    const nextOverlay = imagery.coastlines ? imagery.coastlines.url : '';
    if (nextBase === baseKey && nextOverlay === overlayKey) return;
    // Only the layer that changed is replaced, so toggling the overlay keeps the loaded basemap.
    if (nextBase !== baseKey) {
      if (baseLayer) viewer.imageryLayers.remove(baseLayer, true);
      tiles.loaded = 0; tiles.failed = 0; tiles.cancelled = 0;
      baseLayer = addLayer(imagery.active, 0);
      baseKey = nextBase;
    }
    if (nextOverlay !== overlayKey) {
      if (overlayLayer) viewer.imageryLayers.remove(overlayLayer, true);
      overlayLayer = nextOverlay ? addLayer(imagery.coastlines) : null;
      overlayKey = nextOverlay;
    }
    viewer.scene.requestRender();
  }
  function addLayer(layer, index) {
    const Cesium = window.Cesium;
    const source = layer.source;
    const provider = new Cesium.WebMapTileServiceImageryProvider({
      url: layer.url, layer: source.layer, style: 'default', format: source.format,
      tileMatrixSetID: source.tile_matrix_set, maximumLevel: source.maximum_level,
      credit: new Cesium.Credit(source.attribution)
    });
    const request = provider.requestImage.bind(provider);
    provider.requestImage = (x, y, level, req) => {
      const promise = request(x, y, level, req);
      if (!promise) return promise;
      return promise.then(image => { tiles.loaded += 1; renderAttribution(); return image; },
        error => {
          // A request Cesium cancelled because the tile left the view is navigation, not a gap.
          const cancelled = req?.state === Cesium.RequestState.CANCELLED || /cancelled/i.test(String(error?.message || ''));
          if (cancelled) tiles.cancelled += 1;
          else { tiles.failed += 1; note('tile', `${source.layer} ${level}/${x}/${y}: ${error?.message || error}`); }
          renderAttribution();
          throw error;
        });
    };
    provider.errorEvent.addEventListener(error => note('provider', error?.message || error)); // counted above; keep the console quiet
    return viewer.imageryLayers.addImageryProvider(provider, index);
  }

  // ---- Geometry ----
  function circle(center, radiusM, segments = 72) {
    const Cesium = window.Cesium;
    const frame = Cesium.Transforms.eastNorthUpToFixedFrame(center);
    const points = [];
    for (let i = 0; i < segments; i++) {
      const t = i / segments * Math.PI * 2;
      const local = new Cesium.Cartesian3(Math.cos(t) * radiusM, Math.sin(t) * radiusM, 0);
      const world = Cesium.Matrix4.multiplyByPoint(frame, local, new Cesium.Cartesian3());
      const carto = Cesium.Cartographic.fromCartesian(world);
      points.push(Cesium.Cartesian3.fromRadians(carto.longitude, carto.latitude, 0));
    }
    return points;
  }
  function buildGeometry(plotted) {
    const Cesium = window.Cesium;
    viewer.entities.removeAll();
    drawn.clear();
    const ACCENT = Cesium.Color.fromCssColorString('#8ff0a4');
    const AMBER = Cesium.Color.fromCssColorString('#f3b64a');
    const TEXT = Cesium.Color.fromCssColorString('#e9edf0');
    const NONE = Cesium.Color.TRANSPARENT;
    for (const record of plotted) {
      const g = record.geometry;
      if (g.kind === 'point') {
        const anchor = Cesium.Cartesian3.fromDegrees(g.lon_deg, g.lat_deg, 0);
        viewer.entities.add({ id: record.id, position: anchor, point: { pixelSize: 7, color: ACCENT, outlineColor: Cesium.Color.BLACK, outlineWidth: 1.5, disableDepthTestDistance: Number.POSITIVE_INFINITY } });
        let points = [anchor];
        if (g.horizontal_uncertainty_m) {
          points = circle(anchor, g.horizontal_uncertainty_m);
          viewer.entities.add({ id: `${record.id}:uncertainty`, polyline: { positions: [...points, points[0]], width: 1.5, material: ACCENT.withAlpha(0.8), clampToGround: false } });
        }
        drawn.set(record.id, { anchor, points, kind: 'point', cls: '' });
      } else if (g.kind === 'approximate_centroid') {
        const anchor = Cesium.Cartesian3.fromDegrees(g.lon_deg, g.lat_deg, 0);
        const points = circle(anchor, g.radius_m);
        viewer.entities.add({ id: record.id, position: anchor, point: { pixelSize: 6, color: NONE, outlineColor: AMBER, outlineWidth: 1.5, disableDepthTestDistance: Number.POSITIVE_INFINITY } });
        viewer.entities.add({ id: `${record.id}:radius`, polyline: { positions: [...points, points[0]], width: 1.5, material: new Cesium.PolylineDashMaterialProperty({ color: AMBER, dashLength: 12 }) } });
        drawn.set(record.id, { anchor, points, kind: 'approximate_centroid', cls: 'approx' });
      } else if (g.kind === 'region') {
        const ring = g.ring.map(p => Cesium.Cartesian3.fromDegrees(p.lon_deg, p.lat_deg, 0));
        viewer.entities.add({ id: record.id, polygon: { hierarchy: ring, height: 0, material: new Cesium.StripeMaterialProperty({ evenColor: TEXT.withAlpha(0.16), oddColor: NONE, repeat: 48, orientation: Cesium.StripeOrientation.VERTICAL }), outline: false } });
        viewer.entities.add({ id: `${record.id}:outline`, polyline: { positions: [...ring, ring[0]], width: 1.2, material: new Cesium.PolylineDashMaterialProperty({ color: TEXT.withAlpha(0.7), dashLength: 10 }) } });
        // The tag leads to the northernmost vertex: an edge of the area, never a derived centre.
        let anchor = ring[0], best = -Infinity;
        g.ring.forEach((p, i) => { if (p.lat_deg > best) { best = p.lat_deg; anchor = ring[i]; } });
        drawn.set(record.id, { anchor, points: ring, kind: 'region', cls: 'region' });
      }
    }
    viewer.scene.requestRender();
  }
  function onClick(movement) {
    if (!viewer || !view) return;
    const picked = viewer.scene.pick(movement.position);
    const id = picked?.id?.id;
    const recordId = typeof id === 'string' ? id.split(':')[0] : null;
    if (recordId && drawn.has(recordId)) select(recordId === view.selection ? null : recordId);
  }

  // ---- Overlays: tags, leaders, bracket ----
  function buildTags(plotted) {
    const host = $('globe-tags');
    host.replaceChildren();
    tagEls.clear();
    for (const record of plotted) {
      const d = drawn.get(record.id);
      const tag = node('button', undefined, `gtag ${d ? d.cls : ''}`.trim());
      tag.type = 'button';
      tag.hidden = true;
      const label = record.geometry.kind === 'region' ? `${record.label} · region` : record.geometry.kind === 'approximate_centroid' ? `${record.label} · ≈ place` : record.label;
      tag.append(node('i', pad2(record.order)), node('b', label));
      tag.setAttribute('aria-label', `${record.label}, ${record.geometry.kind === 'region' ? 'reported region' : record.geometry.kind === 'approximate_centroid' ? 'approximate place centroid' : 'recorded point'}`);
      tag.onclick = () => select(view.selection === record.id ? null : record.id);
      host.append(tag);
      tagEls.set(record.id, tag);
    }
  }
  function positionOverlays() {
    if (!viewer || !view) return;
    const Cesium = window.Cesium;
    const scene = viewer.scene;
    const svg = $('globe-leaders');
    const bracket = $('globe-bracket');
    const occluder = new Cesium.EllipsoidalOccluder(Cesium.Ellipsoid.WGS84, scene.camera.positionWC);
    const fragment = document.createDocumentFragment();
    const placed = [];
    let bracketBox = null;
    for (const [id, d] of drawn) {
      const tag = tagEls.get(id);
      if (!tag) continue;
      const win = overlays && occluder.isPointVisible(d.anchor) ? Cesium.SceneTransforms.worldToWindowCoordinates(scene, d.anchor) : undefined;
      const inside = win && win.x >= -40 && win.y >= -40 && win.x <= scene.canvas.clientWidth + 40 && win.y <= scene.canvas.clientHeight + 40;
      if (!inside) { tag.hidden = true; continue; }
      tag.hidden = false;
      tag.setAttribute('aria-pressed', String(view.selection === id));
      const tx = Math.round(win.x + 26);
      let ty = Math.round(win.y - 36);
      // Tags whose anchors fall within a few pixels of each other (zoomed out) are stacked
      // downwards in display order instead of drawn on top of one another.
      const w = tag.offsetWidth, h = tag.offsetHeight;
      for (let attempt = 0; attempt < 12; attempt++) {
        const clash = placed.some(p => tx < p.x + p.w + 4 && tx + w + 4 > p.x && ty < p.y + p.h + 4 && ty + h + 4 > p.y);
        if (!clash) break;
        ty += h + 6;
      }
      placed.push({ x: tx, y: ty, w, h });
      tag.style.transform = `translate(${tx}px, ${ty}px)`;
      const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
      if (d.cls) g.setAttribute('class', d.cls);
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path.setAttribute('d', `M${tx} ${ty + tag.offsetHeight} L${tx} ${Math.round(win.y)} L${Math.round(win.x)} ${Math.round(win.y)}`);
      const dot = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
      dot.setAttribute('cx', Math.round(win.x)); dot.setAttribute('cy', Math.round(win.y)); dot.setAttribute('r', 2);
      g.append(path, dot);
      fragment.append(g);
      if (view.selection === id) {
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        for (const p of d.points) {
          if (!occluder.isPointVisible(p)) continue;
          const w = Cesium.SceneTransforms.worldToWindowCoordinates(scene, p);
          if (!w) continue;
          minX = Math.min(minX, w.x); minY = Math.min(minY, w.y); maxX = Math.max(maxX, w.x); maxY = Math.max(maxY, w.y);
        }
        if (Number.isFinite(minX)) bracketBox = { x: minX - 10, y: minY - 10, w: Math.max(maxX - minX + 20, 28), h: Math.max(maxY - minY + 20, 28) };
      }
    }
    svg.replaceChildren(fragment);
    bracket.hidden = !bracketBox;
    if (bracketBox) {
      bracket.style.transform = `translate(${Math.round(bracketBox.x)}px, ${Math.round(bracketBox.y)}px)`;
      bracket.style.width = `${Math.round(bracketBox.w)}px`;
      bracket.style.height = `${Math.round(bracketBox.h)}px`;
    }
  }

  // ---- Selection ----
  function select(recordId) {
    call('globe_select', { recordId });
  }
  function selectedRecord() {
    if (!view || !view.selection) return null;
    return view.plotted.find(r => r.id === view.selection) || view.not_plotted.find(r => r.id === view.selection) || null;
  }
  function geometryRows(record) {
    if (!record) return [];
    const g = record.geometry;
    if (!g) return [['Position', null], ['Why', record.reason]];
    if (g.kind === 'point') return [['Position', `${deg(g.lat_deg)}, ${deg(g.lon_deg)}`], ['Uncertainty', g.horizontal_uncertainty_m ? `± ${fmt(g.horizontal_uncertainty_m)} m` : null, g.horizontal_uncertainty_m ? 'est' : ''], ['Basis', g.basis]];
    if (g.kind === 'approximate_centroid') return [['Place', g.place_label], ['Centroid', `≈ ${deg(g.lat_deg)}, ${deg(g.lon_deg)}`, 'est'], ['Radius', `${fmt(g.radius_m)} m`, 'est'], ['Basis', g.basis]];
    return [['Region', g.label], ['Vertices', `${g.ring.length} (no centre)`], ['Basis', g.basis]];
  }
  function timeRows(record) {
    const t = record.reported_time;
    const est = t.precision === 'approximate_window' ? 'est' : '';
    return [['Reported time', t.precision === 'not_recorded' ? null : `${t.text}${t.zone ? ` · ${t.zone}` : ''}`, est]];
  }
  function renderCard() {
    const card = $('globe-card');
    const record = selectedRecord();
    if (!record || !record.geometry || !overlays) { card.hidden = true; return; }
    card.hidden = false;
    card.replaceChildren(node('span', undefined, 'corner tl'), node('span', undefined, 'corner tr'), node('span', undefined, 'corner bl'), node('span', undefined, 'corner br'));
    card.append(node('span', `${pad2(record.order)} · selected`, 'k'), node('h3', record.label));
    const rows = node('dl', undefined, 'rows');
    for (const [key, value, cls] of [['Kind', record.kind], ...geometryRows(record).filter(r => r[0] !== 'Basis'), ...timeRows(record), ['Source', record.source_label]]) {
      rows.append(node('dt', key), node('dd', value === null || value === undefined ? '— not recorded' : value, value === null || value === undefined ? 'none' : (cls || '')));
    }
    card.append(rows);
  }
  function renderTray() {
    const tray = $('globe-tray'), host = $('globe-tray-items');
    host.replaceChildren();
    tray.hidden = !view || !view.not_plotted.length;
    if (tray.hidden) return;
    for (const record of view.not_plotted) {
      const tag = node('button', undefined, 'tag');
      tag.type = 'button';
      tag.append(node('i', pad2(record.order)), node('b', `${record.label} · ${record.reason.replace(/\.$/, '')}`));
      tag.setAttribute('aria-pressed', String(view.selection === record.id));
      tag.onclick = () => select(view.selection === record.id ? null : record.id);
      host.append(tag);
    }
  }
  function renderAttribution() {
    const el = $('globe-attrib');
    if (!view) { el.textContent = ''; return; }
    const a = view.imagery.active;
    el.replaceChildren();
    el.append(node('b', 'Imagery '), node('span', `${a.source.provider} · ${a.source.layer}${a.date ? ` · ${a.date} UTC` : ' · composite, no date'}`));
    if (view.imagery.coastlines) el.append(node('span', ` · + ${view.imagery.coastlines.source.layer} (${view.imagery.coastlines.source.attribution})`));
    el.append(node('span', ` · tiles ${fmt(tiles.loaded)} loaded`));
    if (tiles.failed) el.append(node('span', `, ${fmt(tiles.failed)} failed (gap, not empty scene)`, 'warn'));
    if (tiles.pending) el.append(node('span', `, ${fmt(tiles.pending)} pending`));
    if (tiles.cancelled) el.append(node('span', `, ${fmt(tiles.cancelled)} cancelled by navigation`));
    if (a.coverage_note) el.append(node('span', ' · beyond recorded coverage', 'warn'));
    if (rendererState !== 'ready') el.append(node('span', ` · renderer ${rendererState}`, 'warn'));
    notify();
    // The inspector's tile row follows the strip, throttled so a burst of tiles is one rebuild.
    clearTimeout(inspectorTimer);
    inspectorTimer = setTimeout(renderInspector, 250);
  }
  let inspectorTimer = null;
  function renderState() {
    const el = $('globe-view-state');
    el.replaceChildren();
    if (!view) return;
    const cells = [
      ['CASE', view.case ? view.case.id : 'none'],
      ['CLOCK', view.case ? `${view.case.clock.local.slice(11)} ±${view.case.clock.uncertainty_min} min · reported local` : '—'],
      ['IMAGERY', view.imagery.active.source.id],
      ['OVERLAYS', overlays ? 'on' : 'hidden'],
      ['ALT', `${fmt(Math.round(view.camera.height_m / 1000))} km`]
    ];
    for (const [k, v] of cells) { const span = node('span'); span.append(node('b', `${k} `), document.createTextNode(v)); el.append(span); }
  }
  function renderInspector() {
    if (!view) return;
    const record = selectedRecord();
    $('insp-globe-title').textContent = record ? record.label : (view.case ? view.case.label : 'Globe');
    const chips = $('insp-globe-chips');
    chips.replaceChildren();
    const chip = (text, cls) => { const c = node('span', text, `chip${cls ? ` ${cls}` : ''}`); chips.append(c); };
    if (view.case) chip(`◇ ${view.case.origin_class} case`, 'fx');
    if (record) chip(record.geometry ? (record.geometry.kind === 'region' ? '▦ Region' : record.geometry.kind === 'approximate_centroid' ? '◌ Approximate' : '◉ Point') : '○ Not plotted');
    if (rendererState === 'ready' && tiles.loaded > 0) chip('▮ Imagery live from source');
    else chip(`○ Imagery not loaded · renderer ${rendererState}`, 'warn');
    kv($('globe-record'), record ? [['Record', record.label], ['Kind', record.kind], ...geometryRows(record), ...timeRows(record), ['Source', record.source_label]] : [['Record', 'none selected']]);
    $('globe-record-note').textContent = record ? record.notes : 'Select a tag on the globe, a record below, or an entry in the not-plotted tray.';
    const list = $('globe-records');
    list.replaceChildren();
    const all = [...view.plotted.map(r => ({ r, plotted: true })), ...view.not_plotted.map(r => ({ r, plotted: false }))].sort((a, b) => a.r.order - b.r.order);
    for (const { r, plotted } of all) {
      const tag = node('button', undefined, 'tag');
      tag.type = 'button';
      tag.append(node('i', pad2(r.order)), node('b', `${r.label} · ${plotted ? r.geometry.kind.replace('_', ' ') : 'not plotted'}`));
      tag.setAttribute('aria-pressed', String(view.selection === r.id));
      tag.onclick = () => select(view.selection === r.id ? null : r.id);
      list.append(tag);
    }
    if (view.case) {
      kv($('globe-case'), [['Case', view.case.label], ['Origin', view.case.origin_class], ['Interval', `${view.case.interval.start_local.slice(11)}–${view.case.interval.end_local.slice(11)} · ${view.case.interval.zone}`], ['Clock', `${view.case.clock.local.slice(11)} ±${view.case.clock.uncertainty_min} min`, 'est'], ['Records', `${view.case.record_count} · ${view.case.plotted_count} plotted · ${view.case.not_plotted_count} not plotted`], ['Sources', view.case.sources.map(s => s.label).join('; ')], ['Fixture', view.case.fixture_path], ['Publication', null], ['UTC instant', null]]);
    } else {
      kv($('globe-case'), [['Case', 'none'], ['Fixture', view.fixture_error || 'not loaded']]);
    }
    renderImageryForm();
    const a = view.imagery.active;
    kv($('globe-imagery-active'), [['Layer', a.source.layer], ['Provider', a.source.provider], ['Origin', a.source.origin], ['Date', a.date ? `${a.date} (UTC day)` : 'composite · none'], ['Capture', a.source.capture_statement], ['Tiles', `${fmt(tiles.loaded)} loaded · ${fmt(tiles.failed)} failed · ${fmt(tiles.pending)} pending · ${fmt(tiles.cancelled)} cancelled by navigation`, tiles.failed ? 'est' : ''], ['Overlay', view.imagery.coastlines ? `${view.imagery.coastlines.source.layer} · ${view.imagery.coastlines.source.license} · ${view.imagery.coastlines.source.attribution}` : 'none'], ['Coverage note', a.coverage_note || null], ['Licence', a.source.license], ['Attribution', a.source.attribution], ['Checked', `capabilities ${a.source.capabilities_checked_on}`]]);
    $('globe-ack').textContent = view.imagery.acknowledgement;
    kv($('globe-renderer'), [['Renderer', `${view.renderer.name} ${view.renderer.pinned_version} · ${rendererState}`], ['Ion', view.renderer.ion], ['Terrain', view.renderer.terrain], ['Effects', reduced ? 'reduced: no atmosphere or fog' : 'atmosphere and fog (display only)']]);
  }
  function renderImageryForm() {
    const form = $('globe-imagery-form');
    const current = form.dataset.rendered;
    const stamp = JSON.stringify([view.imagery.sources.map(s => s.id), view.imagery.active.source.id, view.imagery.active.date, !!view.imagery.coastlines, view.imagery.today_utc]);
    if (current === stamp) return;
    form.dataset.rendered = stamp;
    form.replaceChildren();
    const basemaps = view.imagery.sources.filter(s => s.role === 'basemap');
    const overlaysList = view.imagery.sources.filter(s => s.role === 'overlay');
    for (const source of basemaps) {
      const label = node('label', undefined, 'row'), input = node('input');
      input.type = 'radio'; input.name = 'source'; input.value = source.id; input.checked = source.id === view.imagery.active.source.id;
      input.onchange = () => syncDate();
      const text = node('span', source.label);
      text.append(node('small', source.time ? `Daily · ${source.time.coverage[0].start} to ${source.time.coverage[source.time.coverage.length - 1].end} with gaps · ${source.capabilities_checked_on}` : 'Composite · no date'));
      label.append(input, text); form.append(label);
    }
    const dateRow = node('label', undefined, 'date');
    dateRow.append(node('span', 'UTC day'));
    const date = node('input'); date.type = 'date'; date.name = 'date'; date.max = view.imagery.today_utc;
    const dated = basemaps.find(s => s.time);
    if (dated) date.min = dated.time.coverage[0].start;
    date.value = view.imagery.active.date || '';
    dateRow.append(date); form.append(dateRow);
    for (const source of overlaysList) {
      const label = node('label', undefined, 'row'), input = node('input');
      input.type = 'checkbox'; input.name = 'coastlines'; input.checked = !!view.imagery.coastlines;
      const text = node('span', source.label); text.append(node('small', source.capture_statement));
      label.append(input, text); form.append(label);
    }
    const apply = node('button', 'Apply imagery', 'btn primary'); apply.type = 'submit';
    form.append(apply, node('p', 'Tiles are requested from the registered origin only; a day the source has not published is refused as a gap.', 'note'));
    const syncDate = () => {
      const chosen = basemaps.find(s => s.id === form.source.value);
      date.disabled = !chosen || !chosen.time;
    };
    syncDate();
    form.onsubmit = event => {
      event.preventDefault();
      const chosen = basemaps.find(s => s.id === form.source.value);
      call('globe_imagery', { selection: { source_id: form.source.value, date: chosen && chosen.time && date.value ? date.value : null, coastlines: !!form.coastlines?.checked } });
    };
  }

  // ---- View application ----
  function applyView(next) {
    const first = !view;
    const previousSelection = view?.selection;
    view = next;
    if (viewer && rendererState === 'ready') {
      setImagery(view.imagery);
      const key = JSON.stringify(view.plotted);
      if (key !== plottedKey) { plottedKey = key; buildGeometry(view.plotted); buildTags(view.plotted); }
      if (first) setCamera(view.camera, false);
      if (view.selection && view.selection !== previousSelection) {
        const d = drawn.get(view.selection);
        if (d) {
          const Cesium = window.Cesium;
          const occluder = new Cesium.EllipsoidalOccluder(Cesium.Ellipsoid.WGS84, viewer.scene.camera.positionWC);
          const win = occluder.isPointVisible(d.anchor) ? Cesium.SceneTransforms.worldToWindowCoordinates(viewer.scene, d.anchor) : undefined;
          const c = viewer.scene.canvas;
          if (!win || win.x < 0 || win.y < 0 || win.x > c.clientWidth || win.y > c.clientHeight) {
            const carto = Cesium.Cartographic.fromCartesian(d.anchor);
            setCamera({ lat_deg: Cesium.Math.toDegrees(carto.latitude), lon_deg: Cesium.Math.toDegrees(carto.longitude), height_m: Math.max(viewer.camera.positionCartographic.height, 5000), heading_deg: 0, pitch_deg: -90 }, true);
          }
        }
      }
      viewer.scene.requestRender();
    }
    renderCard(); renderTray(); renderAttribution(); renderState(); renderInspector();
    if (onViewListener) onViewListener(summary());
  }
  function summary() {
    if (!view) return { source: '—', coverage: '—', selection: '—', loaded: false };
    const record = selectedRecord();
    return {
      source: view.case ? `${view.case.label}` : 'no case',
      coverage: rendererState === 'ready' ? `tiles ${fmt(tiles.loaded)} ok · ${fmt(tiles.failed)} failed` : `renderer ${rendererState}`,
      partial: tiles.failed > 0 || rendererState !== 'ready',
      selection: record ? `${pad2(record.order)} ${record.label}` : 'none',
      loaded: true
    };
  }

  // ---- Public surface used by app.js ----
  async function activate() {
    active = true;
    if (!view) await call('globe_view', {});
    if (rendererState === 'idle') loadCesium();
    if (viewer) { viewer.resize(); viewer.scene.requestRender(); }
  }
  function deactivate() { active = false; }
  function setReduced(on) {
    reduced = on;
    if (!viewer) return;
    viewer.scene.skyAtmosphere.show = !on;
    viewer.scene.globe.showGroundAtmosphere = !on;
    viewer.scene.fog.enabled = !on;
    viewer.scene.requestRender();
    renderInspector();
  }
  function toggleOverlays(force) {
    overlays = force === undefined ? !overlays : !!force;
    $('globe-overlays').setAttribute('aria-pressed', String(!overlays));
    if (viewer) { viewer.entities.show = overlays; viewer.scene.requestRender(); }
    renderCard(); renderState();
    if (!overlays) { $('globe-leaders').replaceChildren(); $('globe-bracket').hidden = true; tagEls.forEach(t => { t.hidden = true; }); }
  }
  function onKey(event) {
    if (!active || !viewer || rendererState !== 'ready') return false;
    const inCanvas = $('globe-canvas').contains(document.activeElement) || document.activeElement === $('globe-canvas');
    if (!inCanvas) return false;
    const map = { ArrowLeft: () => pan(-1, 0), ArrowRight: () => pan(1, 0), ArrowUp: () => pan(0, 1), ArrowDown: () => pan(0, -1), '+': () => zoom(1), '=': () => zoom(1), '-': () => zoom(-1), Home: home, '0': world };
    const action = map[event.key];
    if (!action) return false;
    event.preventDefault();
    action();
    return true;
  }
  function wire() {
    $('globe-home').onclick = home;
    $('globe-world').onclick = world;
    $('globe-overlays').onclick = () => toggleOverlays();
    $('globe-canvas').addEventListener('keydown', event => {
      if (event.key === 'Escape' && view?.selection) { event.preventDefault(); select(null); return; }
      onKey(event);
    });
    window.addEventListener('resize', () => { if (viewer && active) viewer.resize(); });
  }
  wire();
  return { activate, deactivate, setReduced, toggleOverlays, summary, onView: fn => { onViewListener = fn; }, refresh: () => call('globe_view', {}) };
})();
