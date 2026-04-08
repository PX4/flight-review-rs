<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { PlotConfig, FlightMetadata } from '$lib/types';
	import { activePlots, togglePlotMinimized } from '$lib/stores/logViewer';
	import { initDuckDB, LogSession } from '$lib/utils/duckdb';

	let { config, logId, metadata, index, totalCount, onMoveUp, onMoveDown, onDragStart, onDragEnd } = $props<{
		config: PlotConfig;
		logId: string;
		metadata: FlightMetadata;
		index?: number;
		totalCount?: number;
		onMoveUp?: () => void;
		onMoveDown?: () => void;
		onDragStart?: (e: DragEvent) => void;
		onDragEnd?: (e: DragEvent) => void;
	}>();

	const sessionCache = (globalThis as any).__plotSessionCache ??= new Map<string, LogSession>();

	let containerEl: HTMLDivElement | undefined = $state();
	let canvasEl: HTMLCanvasElement | undefined = $state();
	let resizeObserver: ResizeObserver | null = null;

	let loading = $state(true);
	let error = $state<string | null>(null);
	let plotHeight = $state(460);
	let lastWidth = 0;

	// Cached series data (raw, in north/east meters). Re-rendered on resize.
	type Series = { east: number[]; north: number[] };
	let estSeries: Series | null = null;
	let spSeries: Series | null = null;
	let gpsSeries: Series | null = null;
	let markerSeries: Series | null = null;

	const COLOR_ESTIMATED = '#f97316'; // orange
	const COLOR_SETPOINT = '#10b981'; // emerald
	const COLOR_GPS = '#3b82f6'; // blue
	const COLOR_SP_MARKER = '#ec4899'; // pink

	const LEGEND = [
		{ label: 'Estimated', color: COLOR_ESTIMATED },
		{ label: 'Setpoint', color: COLOR_SETPOINT },
		{ label: 'GPS (projected)', color: COLOR_GPS },
		{ label: 'Position Setpoints', color: COLOR_SP_MARKER, marker: true },
	];

	async function getSession(): Promise<LogSession> {
		if (sessionCache.has(logId)) return sessionCache.get(logId)!;
		const db = await initDuckDB();
		const session = new LogSession(db, logId);
		sessionCache.set(logId, session);
		return session;
	}

	function removePlot() {
		activePlots.update((plots) => plots.filter((p) => p.id !== config.id));
	}

	function downloadPng() {
		if (!canvasEl) return;
		const link = document.createElement('a');
		link.download = `trajectory.png`;
		link.href = canvasEl.toDataURL('image/png');
		link.click();
	}

	/**
	 * Project lat/lon (degrees) to local NE meters using an equirectangular
	 * projection around a reference latitude. Returns [north_m, east_m] in the
	 * PX4 NED convention so it lines up with vehicle_local_position (x=N, y=E).
	 */
	function projectLatLon(
		lat: number, lon: number, refLat: number, refLon: number
	): [number, number] {
		const R = 6378137; // WGS-84 equatorial radius
		const refLatRad = (refLat * Math.PI) / 180;
		const dLat = ((lat - refLat) * Math.PI) / 180;
		const dLon = ((lon - refLon) * Math.PI) / 180;
		const north = dLat * R;
		const east = dLon * R * Math.cos(refLatRad);
		return [north, east];
	}

	/**
	 * Compute the bounding box of all visible series, with equal aspect (so the
	 * trajectory isn't distorted) and a small margin.
	 */
	function computeBounds(): { eMin: number; eMax: number; nMin: number; nMax: number } | null {
		let eMin = Infinity, eMax = -Infinity, nMin = Infinity, nMax = -Infinity;
		const all = [estSeries, spSeries, gpsSeries, markerSeries].filter((s): s is Series => !!s);
		if (all.length === 0) return null;
		for (const s of all) {
			for (let i = 0; i < s.east.length; i++) {
				const e = s.east[i], n = s.north[i];
				if (!Number.isFinite(e) || !Number.isFinite(n)) continue;
				if (e < eMin) eMin = e;
				if (e > eMax) eMax = e;
				if (n < nMin) nMin = n;
				if (n > nMax) nMax = n;
			}
		}
		if (!Number.isFinite(eMin)) return null;
		// Add 5% margin and equalize aspect so the shape isn't distorted.
		const eRange = eMax - eMin || 1;
		const nRange = nMax - nMin || 1;
		const margin = 0.05;
		eMin -= eRange * margin; eMax += eRange * margin;
		nMin -= nRange * margin; nMax += nRange * margin;
		return { eMin, eMax, nMin, nMax };
	}

	/** Pick "nice" tick values for an axis range. */
	function niceTicks(min: number, max: number, target: number): number[] {
		const range = max - min;
		if (range <= 0) return [min];
		const rough = range / target;
		const pow = Math.pow(10, Math.floor(Math.log10(rough)));
		const norm = rough / pow;
		let step;
		if (norm < 1.5) step = pow;
		else if (norm < 3) step = 2 * pow;
		else if (norm < 7) step = 5 * pow;
		else step = 10 * pow;
		const start = Math.ceil(min / step) * step;
		const ticks: number[] = [];
		for (let v = start; v <= max + step * 0.5; v += step) {
			ticks.push(Math.round(v / step) * step);
		}
		return ticks;
	}

	function render(widthOverride?: number) {
		const canvas = canvasEl;
		if (!canvas || !containerEl) return;
		const bounds = computeBounds();
		if (!bounds) return;

		const dpr = window.devicePixelRatio || 1;
		const cssW = widthOverride ?? lastWidth ?? containerEl.getBoundingClientRect().width;
		if (!cssW || cssW <= 0) return;
		lastWidth = cssW;
		const cssH = plotHeight;

		canvas.style.width = `${cssW}px`;
		canvas.style.height = `${cssH}px`;
		canvas.width = Math.floor(cssW * dpr);
		canvas.height = Math.floor(cssH * dpr);

		const ctx = canvas.getContext('2d');
		if (!ctx) return;
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, cssW, cssH);

		// Layout: padding for axes labels.
		const padL = 56, padR = 16, padT = 12, padB = 36;
		const plotW = cssW - padL - padR;
		const plotH = cssH - padT - padB;
		if (plotW <= 0 || plotH <= 0) return;

		// Fill the plot area on both axes (independent X/Y scaling). Aspect is
		// not preserved — the trajectory shape stretches to fit, like the other
		// time-series plots on the page.
		let { eMin, eMax, nMin, nMax } = bounds;
		const dataE = eMax - eMin || 1;
		const dataN = nMax - nMin || 1;
		const scaleX = plotW / dataE;
		const scaleY = plotH / dataN;
		const offsetX = padL;
		const offsetY = padT;
		const drawW = plotW;
		const drawH = plotH;

		const toPx = (e: number, n: number): [number, number] => {
			const x = offsetX + (e - eMin) * scaleX;
			// Y is inverted: north up means smaller pixel y.
			const y = offsetY + (nMax - n) * scaleY;
			return [x, y];
		};

		// --- Grid + axes ---
		ctx.strokeStyle = '#e5e7eb';
		ctx.lineWidth = 1;
		ctx.fillStyle = '#9ca3af';
		ctx.font = '11px ui-sans-serif, system-ui, sans-serif';

		const eTicks = niceTicks(eMin, eMax, Math.max(4, Math.floor(plotW / 80)));
		const nTicks = niceTicks(nMin, nMax, Math.max(4, Math.floor(plotH / 50)));

		ctx.beginPath();
		for (const t of eTicks) {
			const [x] = toPx(t, 0);
			if (x < offsetX - 0.5 || x > offsetX + drawW + 0.5) continue;
			ctx.moveTo(x, offsetY);
			ctx.lineTo(x, offsetY + drawH);
		}
		for (const t of nTicks) {
			const [, y] = toPx(0, t);
			if (y < offsetY - 0.5 || y > offsetY + drawH + 0.5) continue;
			ctx.moveTo(offsetX, y);
			ctx.lineTo(offsetX + drawW, y);
		}
		ctx.stroke();

		// Tick labels
		ctx.textAlign = 'center';
		ctx.textBaseline = 'top';
		for (const t of eTicks) {
			const [x] = toPx(t, 0);
			if (x < offsetX - 0.5 || x > offsetX + drawW + 0.5) continue;
			ctx.fillText(String(Math.round(t)), x, offsetY + drawH + 4);
		}
		ctx.textAlign = 'right';
		ctx.textBaseline = 'middle';
		for (const t of nTicks) {
			const [, y] = toPx(0, t);
			if (y < offsetY - 0.5 || y > offsetY + drawH + 0.5) continue;
			ctx.fillText(String(Math.round(t)), offsetX - 6, y);
		}

		// Axis labels
		ctx.textAlign = 'center';
		ctx.textBaseline = 'alphabetic';
		ctx.fillText('East [m]', offsetX + drawW / 2, cssH - 6);
		ctx.save();
		ctx.translate(14, offsetY + drawH / 2);
		ctx.rotate(-Math.PI / 2);
		ctx.fillText('North [m]', 0, 0);
		ctx.restore();

		// Border
		ctx.strokeStyle = '#d1d5db';
		ctx.strokeRect(offsetX, offsetY, drawW, drawH);

		// --- Polylines ---
		const drawLine = (s: Series | null, color: string, width: number) => {
			if (!s || s.east.length < 2) return;
			ctx.strokeStyle = color;
			ctx.lineWidth = width;
			ctx.lineJoin = 'round';
			ctx.lineCap = 'round';
			ctx.beginPath();
			let started = false;
			for (let i = 0; i < s.east.length; i++) {
				const e = s.east[i], n = s.north[i];
				if (!Number.isFinite(e) || !Number.isFinite(n)) { started = false; continue; }
				const [x, y] = toPx(e, n);
				if (!started) { ctx.moveTo(x, y); started = true; }
				else ctx.lineTo(x, y);
			}
			ctx.stroke();
		};

		drawLine(gpsSeries, COLOR_GPS, 1.5);
		drawLine(estSeries, COLOR_ESTIMATED, 1.75);
		drawLine(spSeries, COLOR_SETPOINT, 1.75);

		// --- Marker rings ---
		if (markerSeries) {
			ctx.strokeStyle = COLOR_SP_MARKER;
			ctx.lineWidth = 2;
			for (let i = 0; i < markerSeries.east.length; i++) {
				const e = markerSeries.east[i], n = markerSeries.north[i];
				if (!Number.isFinite(e) || !Number.isFinite(n)) continue;
				const [x, y] = toPx(e, n);
				ctx.beginPath();
				ctx.arc(x, y, 5, 0, Math.PI * 2);
				ctx.stroke();
			}
		}
	}

	async function loadAndRender() {
		loading = true;
		error = null;

		try {
			const session = await getSession();
			const topics = new Set(Object.keys(metadata.topics));

			// --- Estimated trajectory (vehicle_local_position) ---
			let estX: Float64Array | null = null;
			let estY: Float64Array | null = null;
			let refLat: number | null = null;
			let refLon: number | null = null;
			if (topics.has('vehicle_local_position')) {
				const lpSchema = await session.getTopicSchema('vehicle_local_position');
				const lpNames = new Set(lpSchema.map((c) => c.name));
				const hasRef = lpNames.has('ref_lat') && lpNames.has('ref_lon');
				const fields = hasRef ? ['x', 'y', 'ref_lat', 'ref_lon'] : ['x', 'y'];
				const r = await session.queryTopic('vehicle_local_position', fields);
				if (r) {
					estX = r.series[0];
					estY = r.series[1];
					if (hasRef) {
						// Use the first non-zero ref_lat/lon as the projection origin.
						for (let i = 0; i < r.series[2].length; i++) {
							if (r.series[2][i] !== 0 && r.series[3][i] !== 0) {
								refLat = r.series[2][i];
								refLon = r.series[3][i];
								break;
							}
						}
					}
				}
			}

			// Fallback projection origin: first valid GPS fix from metadata.
			if ((refLat == null || refLon == null) && metadata.analysis?.gps_track?.length) {
				const first = metadata.analysis.gps_track[0];
				refLat = first.lat_deg;
				refLon = first.lon_deg;
			}

			// --- Setpoint trajectory (vehicle_local_position_setpoint) ---
			let spX: Float64Array | null = null;
			let spY: Float64Array | null = null;
			if (topics.has('vehicle_local_position_setpoint')) {
				const r = await session.queryTopic('vehicle_local_position_setpoint', ['x', 'y']);
				if (r) {
					spX = r.series[0];
					spY = r.series[1];
				}
			}

			// --- GPS projected (vehicle_gps_position) ---
			// Field names vary by firmware: newer logs use latitude_deg/longitude_deg
			// (already in degrees), older ones use lat/lon (1e7 integers).
			let gpsN: number[] = [];
			let gpsE: number[] = [];
			if (topics.has('vehicle_gps_position') && refLat != null && refLon != null) {
				const gpsSchema = await session.getTopicSchema('vehicle_gps_position');
				const names = new Set(gpsSchema.map((c) => c.name));
				let latField: string | null = null;
				let lonField: string | null = null;
				let degScale = 1;
				if (names.has('latitude_deg') && names.has('longitude_deg')) {
					latField = 'latitude_deg';
					lonField = 'longitude_deg';
				} else if (names.has('lat') && names.has('lon')) {
					latField = 'lat';
					lonField = 'lon';
					degScale = 1e-7; // older PX4: int32 * 1e-7
				}
				if (latField && lonField) {
					const r = await session.queryTopic('vehicle_gps_position', [latField, lonField]);
					if (r) {
						const lats = r.series[0];
						const lons = r.series[1];
						for (let i = 0; i < lats.length; i++) {
							const lat = lats[i] * degScale;
							const lon = lons[i] * degScale;
							if (lat === 0 && lon === 0) continue;
							if (!Number.isFinite(lat) || !Number.isFinite(lon)) continue;
							const [n, e] = projectLatLon(lat, lon, refLat, refLon);
							gpsN.push(n);
							gpsE.push(e);
						}
					}
				}
			}

			// --- Position setpoint markers (position_setpoint_triplet.current) ---
			// Field names vary across firmware versions; probe the schema and pick
			// whichever XY-style or lat/lon fields are present.
			let markerN: number[] = [];
			let markerE: number[] = [];
			if (topics.has('position_setpoint_triplet') && refLat != null && refLon != null) {
				const tripletSchema = await session.getTopicSchema('position_setpoint_triplet');
				const names = new Set(tripletSchema.map((c) => c.name));
				const tryPairs: Array<{ x: string; y: string; mode: 'xy' | 'latlon' }> = [
					{ x: 'current.x', y: 'current.y', mode: 'xy' },
					{ x: 'current.lat', y: 'current.lon', mode: 'latlon' },
				];
				for (const pair of tryPairs) {
					if (!names.has(pair.x) || !names.has(pair.y)) continue;
					const r = await session.queryTopic('position_setpoint_triplet', [pair.x, pair.y]);
					if (!r) continue;
					const xs = r.series[0];
					const ys = r.series[1];
					let lastX = NaN;
					let lastY = NaN;
					for (let i = 0; i < xs.length; i++) {
						if (xs[i] === lastX && ys[i] === lastY) continue;
						lastX = xs[i];
						lastY = ys[i];
						if (!Number.isFinite(xs[i]) || !Number.isFinite(ys[i])) continue;
						if (pair.mode === 'latlon') {
							if (xs[i] === 0 && ys[i] === 0) continue;
							const [n, e] = projectLatLon(xs[i], ys[i], refLat, refLon);
							markerN.push(n);
							markerE.push(e);
						} else {
							markerN.push(xs[i]);
							markerE.push(ys[i]);
						}
					}
					break;
				}
			}

			// Convert Float64Array slices into plain arrays of (east, north).
			// Note: PX4 NED convention has x=North, y=East. We display east on the
			// horizontal axis and north on the vertical axis (top-down map view).
			estSeries = null;
			spSeries = null;
			gpsSeries = null;
			markerSeries = null;
			if (estX && estY && estX.length > 0) {
				estSeries = { east: Array.from(estY), north: Array.from(estX) };
			}
			if (spX && spY && spX.length > 0) {
				spSeries = { east: Array.from(spY), north: Array.from(spX) };
			}
			if (gpsE.length > 0) {
				gpsSeries = { east: gpsE, north: gpsN };
			}
			if (markerE.length > 0) {
				markerSeries = { east: markerE, north: markerN };
			}

			if (!estSeries && !spSeries && !gpsSeries && !markerSeries) {
				// Nothing to plot — auto-remove.
				removePlot();
				return;
			}

			loading = false;
			// Wait for the canvas to mount, then draw. Retry on next frame if the
			// container hasn't been laid out yet (width=0).
			const tryRender = () => {
				const w = containerEl?.getBoundingClientRect().width ?? 0;
				if (w > 0) {
					render(w);
				} else {
					requestAnimationFrame(tryRender);
				}
			};
			requestAnimationFrame(tryRender);
		} catch (e) {
			console.error('TrajectoryPlot load error:', e);
			error = e instanceof Error ? e.message : 'Failed to load trajectory data';
			loading = false;
		}
	}

	onMount(() => {
		if (containerEl) {
			resizeObserver = new ResizeObserver((entries) => {
				for (const entry of entries) {
					const w = entry.contentRect.width;
					if (w > 0) {
						plotHeight = w < 640 ? 280 : 460;
						render(w);
					}
				}
			});
			resizeObserver.observe(containerEl);
		}
		loadAndRender();
	});

	onDestroy(() => {
		if (resizeObserver) {
			resizeObserver.disconnect();
			resizeObserver = null;
		}
	});
</script>

<div class="rounded-lg ring-1 ring-gray-200 bg-white overflow-hidden" bind:this={containerEl}>
	<div class="flex items-center justify-between px-2 sm:px-4 py-2 sm:py-2.5 border-b border-gray-100">
		<div class="flex flex-wrap items-center gap-2 sm:gap-4">
			{#if onDragStart}
				<div
					class="hidden md:flex items-center cursor-grab active:cursor-grabbing text-gray-300 hover:text-gray-500"
					draggable="true"
					ondragstart={onDragStart}
					ondragend={onDragEnd}
					role="button"
					tabindex="0"
					aria-label="Drag to reorder"
				>
					<svg class="size-5" viewBox="0 0 20 20" fill="currentColor">
						<circle cx="7" cy="4" r="1.5" /><circle cx="13" cy="4" r="1.5" />
						<circle cx="7" cy="10" r="1.5" /><circle cx="13" cy="10" r="1.5" />
						<circle cx="7" cy="16" r="1.5" /><circle cx="13" cy="16" r="1.5" />
					</svg>
				</div>
				<div class="flex md:hidden flex-col -space-y-0.5">
					<button
						class="text-gray-400 hover:text-gray-600 disabled:opacity-30 disabled:cursor-default"
						onclick={onMoveUp}
						disabled={index === 0}
						aria-label="Move plot up"
					>
						<svg class="size-3.5" viewBox="0 0 20 20" fill="currentColor">
							<path fill-rule="evenodd" d="M10 3.293l-6.354 6.353a1 1 0 001.415 1.414L10 6.121l4.939 4.939a1 1 0 001.414-1.414L10 3.293z" clip-rule="evenodd" />
						</svg>
					</button>
					<button
						class="text-gray-400 hover:text-gray-600 disabled:opacity-30 disabled:cursor-default"
						onclick={onMoveDown}
						disabled={index === (totalCount ?? 1) - 1}
						aria-label="Move plot down"
					>
						<svg class="size-3.5" viewBox="0 0 20 20" fill="currentColor">
							<path fill-rule="evenodd" d="M10 16.707l6.354-6.353a1 1 0 00-1.415-1.414L10 13.879l-4.939-4.939a1 1 0 00-1.414 1.414L10 16.707z" clip-rule="evenodd" />
						</svg>
					</button>
				</div>
			{/if}
			<span class="text-xs sm:text-sm font-medium text-gray-900">{config.yLabel || '2D Trajectory'}</span>
			<div class="flex flex-wrap items-center gap-x-1.5 sm:gap-x-3 gap-y-1 text-xs">
				{#each LEGEND as item}
					<span class="flex items-center gap-1.5">
						{#if item.marker}
							<span class="inline-block w-2.5 h-2.5 rounded-full border-2" style="border-color: {item.color};"></span>
						{:else}
							<span class="w-3 h-0.5 rounded" style="background-color: {item.color};"></span>
						{/if}
						<span class="text-gray-500">{item.label}</span>
					</span>
				{/each}
			</div>
		</div>
		<div class="flex items-center gap-1">
			<button class="text-gray-400 hover:text-gray-600" onclick={downloadPng} aria-label="Download as PNG">
				<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12M12 16.5V3" />
				</svg>
			</button>
			<button class="text-gray-400 hover:text-gray-600" onclick={() => togglePlotMinimized(config.id)} aria-label={config.minimized ? 'Expand plot' : 'Minimize plot'}>
				<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
					{#if config.minimized}
						<path stroke-linecap="round" stroke-linejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5" />
					{:else}
						<path stroke-linecap="round" stroke-linejoin="round" d="M4.5 15.75l7.5-7.5 7.5 7.5" />
					{/if}
				</svg>
			</button>
			<button class="text-gray-400 hover:text-gray-600" onclick={removePlot} aria-label="Remove plot">
				<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
				</svg>
			</button>
		</div>
	</div>
	<div
		class="relative bg-gray-50 transition-all duration-200 ease-in-out"
		style="min-height: {config.minimized ? 0 : plotHeight}px; max-height: {config.minimized ? '0px' : 'none'}; overflow: {config.minimized ? 'hidden' : 'visible'};"
	>
		{#if loading}
			<div class="absolute inset-0 flex items-center justify-center">
				<svg class="size-6 animate-spin text-gray-400" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
					<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
					<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
				</svg>
				<span class="ml-2 text-sm text-gray-400">Loading trajectory...</span>
			</div>
		{:else if error}
			<div class="absolute inset-0 flex items-center justify-center">
				<div class="text-center">
					<svg class="size-8 text-red-300 mx-auto mb-2" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
					</svg>
					<p class="text-sm text-red-500">{error}</p>
				</div>
			</div>
		{/if}
		<canvas bind:this={canvasEl} class="block w-full"></canvas>
	</div>
</div>
