'use strict';
// Cesium rendering adapter for the Rust-registered Terrarium source. Cesium's heightmap
// structure decodes the RGB encoding; no evidence height, analysis or geoid conversion
// is computed here. Only visible tiles are fetched, with no queue or synthetic fallback.
const GlobeTerrain = (() => {
  function create(viewer, changed, note) {
    const C = window.Cesium;
    let current = null, generation = 0, settings = null;
    const controllers = new Set();
    const empty = () => ({ loaded: 0, failed: 0, cancelled: 0, pending: 0, deepest: null, lastReceived: null });
    let counts = empty();

    async function pixels(source, x, y, level, request, epoch, observed) {
      const controller = new AbortController();
      controllers.add(controller);
      observed.pending++;
      const timer = setTimeout(() => controller.abort('deadline'), source.deadline_ms);
      // Custom fetches do not enter Cesium's RequestScheduler, which normally observes
      // Request.cancelled. Honour that flag here as well as our source-generation fence.
      const checkCancelled = () => { if (request?.cancelled || request?.state === C.RequestState.CANCELLED) controller.abort('navigation'); };
      const navigationTimer = request ? setInterval(checkCancelled, 50) : null;
      checkCancelled();
      let bitmap;
      try {
        const url = source.url_template.replace('{z}', level).replace('{x}', x).replace('{y}', y);
        const response = await fetch(url, { signal: controller.signal, credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer' });
        if (!response.ok) throw new Error(`Terrain HTTP ${response.status}`);
        const declared = Number(response.headers.get('content-length'));
        if (declared > source.max_tile_bytes) throw new Error('Terrain tile exceeds the byte limit');
        const reader = response.body.getReader();
        const parts = [];
        let bytes = 0;
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          bytes += value.byteLength;
          if (bytes > source.max_tile_bytes) { await reader.cancel(); throw new Error('Terrain tile exceeds the byte limit'); }
          parts.push(value);
        }
        const blob = new Blob(parts, { type: 'image/png' });
        const header = new DataView(await blob.slice(0, 24).arrayBuffer());
        // Refuse invalid dimensions before asking the browser to allocate decoded pixels.
        if (header.byteLength < 24 || header.getUint32(0) !== 0x89504e47 || header.getUint32(4) !== 0x0d0a1a0a || header.getUint32(12) !== 0x49484452 || header.getUint32(16) !== source.tile_size || header.getUint32(20) !== source.tile_size) throw new Error('Terrain tile is not the registered 256 × 256 PNG');
        bitmap = await new Promise((resolve, reject) => {
          const abort = () => reject(new DOMException('Terrain decode cancelled', 'AbortError'));
          const signal = controller.signal;
          signal.addEventListener('abort', abort, { once: true });
          if (signal.aborted) abort();
          createImageBitmap(blob, { colorSpaceConversion: 'none', premultiplyAlpha: 'none' }).then(image => {
            signal.removeEventListener('abort', abort);
            if (signal.aborted) image.close(); else resolve(image);
          }, error => { signal.removeEventListener('abort', abort); reject(error); });
        });
        if (bitmap.width !== source.tile_size || bitmap.height !== source.tile_size) throw new Error('Decoded terrain dimensions differ from the registered grid');
        if (epoch !== generation || controller.signal.aborted) throw new Error('Terrain request superseded or cancelled');
        const canvas = document.createElement('canvas');
        canvas.width = source.tile_size; canvas.height = source.tile_size;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) throw new Error('Terrain pixel decoder is unavailable');
        context.drawImage(bitmap, 0, 0);
        const rgba = context.getImageData(0, 0, canvas.width, canvas.height).data;
        // Transparent/no-data pixels must not become a plausible zero or ocean elevation.
        for (let i = 3; i < rgba.length; i += 4) if (rgba[i] !== 255) throw new Error('Terrain tile contains missing pixels');
        checkCancelled();
        if (epoch !== generation || controller.signal.aborted) throw new Error('Terrain request superseded or cancelled');
        observed.loaded++;
        observed.lastReceived = new Date().toISOString();
        observed.deepest = Math.max(observed.deepest ?? 0, level);
        return new C.HeightmapTerrainData({
          buffer: new Uint8Array(rgba.buffer), width: source.tile_size, height: source.tile_size,
          childTileMask: level < source.maximum_level ? 15 : 0,
          structure: { heightScale: 1 / 256, heightOffset: -32768, elementsPerHeight: 3, stride: 4, elementMultiplier: 256, isBigEndian: true }
        });
      } catch (error) {
        const cancelled = epoch !== generation || (controller.signal.aborted && controller.signal.reason !== 'deadline');
        if (cancelled) observed.cancelled++;
        else { observed.failed++; note('terrain', `${level}/${x}/${y}: ${controller.signal.reason === 'deadline' ? '10 s deadline exceeded' : error.message}`); }
        throw error;
      } finally {
        clearTimeout(timer); clearInterval(navigationTimer); controller.abort('finished'); bitmap?.close(); controllers.delete(controller); observed.pending--;
        if (epoch === generation) { changed(); viewer.scene.requestRender(); }
      }
    }

    function apply(terrain) {
      settings = terrain;
      viewer.scene.verticalExaggeration = terrain.selection.enabled ? terrain.selection.exaggeration : 1;
      viewer.scene.verticalExaggerationRelativeHeight = 0;
      const key = terrain.selection.enabled ? terrain.source.id : 'ellipsoid';
      if (key === current) return;
      current = key; generation++;
      for (const controller of controllers) controller.abort('superseded');
      counts = empty();
      if (!terrain.selection.enabled) viewer.terrainProvider = new C.EllipsoidTerrainProvider();
      else {
        const source = terrain.source, epoch = generation, observed = counts;
        const provider = new C.CustomHeightmapTerrainProvider({
          width: source.tile_size, height: source.tile_size,
          tilingScheme: new C.WebMercatorTilingScheme(), credit: source.attribution,
          callback: () => { throw new Error('Use the registered Terrarium geometry adapter'); }
        });
        provider.getTileDataAvailable = (x, y, level) => level <= source.maximum_level;
        provider.requestTileGeometry = (x, y, level, request) => {
          if (epoch !== generation || observed.pending >= source.max_requests_in_flight) return undefined;
          if (![x, y, level].every(Number.isSafeInteger) || level < 0 || level > source.maximum_level || x < 0 || y < 0 || x >= 2 ** level || y >= 2 ** level) return Promise.reject(new Error('Terrain tile outside registered bounds'));
          return pixels(source, x, y, level, request, epoch, observed);
        };
        provider.errorEvent.addEventListener(error => { error.retry = false; });
        viewer.terrainProvider = provider;
      }
      viewer.scene.requestRender();
    }

    function summary() {
      if (!settings?.selection.enabled) return 'off · WGS84 ellipsoid';
      return `${settings.selection.exaggeration}× display · ${counts.loaded} loaded · ${counts.pending} pending · ${counts.failed} failed (gaps) · ${counts.cancelled} cancelled${counts.deepest === null ? '' : ` · deepest z${counts.deepest}`}`;
    }

    function surfacePoint(point) {
      if (!settings?.selection.enabled) return point;
      const p = C.Cartographic.fromCartesian(point);
      const height = viewer.scene.globe.getHeight(p);
      if (!Number.isFinite(height)) return undefined;
      // getHeight reads Cesium's rendered mesh, which already includes exaggeration.
      // Screen projection only; the original point remains untouched in GlobeView/drawn.
      return C.Cartesian3.fromRadians(p.longitude, p.latitude, height);
    }
    return { apply, summary, surfacePoint, retrievalTime: () => counts.lastReceived, hasFailures: () => !!settings?.selection.enabled && counts.failed > 0 };
  }
  return { create };
})();
