<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { PlotConfig, FlightMetadata } from '$lib/types';
	import { activePlots, togglePlotMinimized } from '$lib/stores/logViewer';
	import { initDuckDB, LogSession } from '$lib/utils/duckdb';
	import { computePsdSpectrogram, sumPsd, psdToDb, type SpectrogramResult } from '$lib/utils/spectrogram';
	import { viridis } from '$lib/utils/viridis';

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
	let plotHeight = $state(300);
	let lastWidth = 0;

	// Computed once on data load, re-rendered on resize
	let spec: SpectrogramResult | null = null;
	let dbMin = 0;
	let dbMax = 0;
	let xRangeStart = 0; // seconds offset (relative to log start)

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
		link.download = `spectrogram.png`;
		link.href = canvasEl.toDataURL('image/png');
		link.click();
	}

	async function loadAndRender() {
		loading = true;
		error = null;

		try {
			const session = await getSession();
			const topics = new Set(Object.keys(metadata.topics));
			if (!topics.has('sensor_combined')) {
				removePlot();
				return;
			}

			const fields = ['accelerometer_m_s2[0]', 'accelerometer_m_s2[1]', 'accelerometer_m_s2[2]'];
			const r = await session.queryTopic('sensor_combined', fields);
			if (!r || r.timestamps.length < 256) {
				removePlot();
				return;
			}

			// Sampling frequency: total span / number of samples (matches v1).
			const ts = r.timestamps;
			const totalSpan = ts[ts.length - 1] - ts[0]; // seconds
			if (totalSpan <= 0) {
				removePlot();
				return;
			}
			const fs = ts.length / totalSpan;
			if (fs < 100) {
				// v1 also drops the plot below 100 Hz
				removePlot();
				return;
			}

			// Compute per-axis spectrograms then sum the PSDs.
			const specs = r.series
				.map((s) => computePsdSpectrogram(s, fs, 256, 128))
				.filter((s): s is SpectrogramResult => s != null);
			if (specs.length === 0) {
				removePlot();
				return;
			}
			const summed = sumPsd(specs);
			if (!summed) {
				removePlot();
				return;
			}
			const range = psdToDb(summed.psd);
			spec = summed;
			dbMin = range.min;
			dbMax = range.max;
			xRangeStart = ts[0];

			loading = false;
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
			console.error('SpectrogramPlot load error:', e);
			error = e instanceof Error ? e.message : 'Failed to load spectrogram data';
			loading = false;
		}
	}

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

	function formatTimeTick(seconds: number): string {
		if (seconds < 60) return `${seconds.toFixed(0)}s`;
		const m = Math.floor(seconds / 60);
		const s = Math.floor(seconds % 60);
		return `${m}:${s.toString().padStart(2, '0')}`;
	}

	function render(widthOverride?: number) {
		const canvas = canvasEl;
		if (!canvas || !containerEl || !spec) return;

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

		// Layout: padding for axes labels and the color bar on the right.
		const padL = 56, padR = 78, padT = 12, padB = 32;
		const plotW = cssW - padL - padR;
		const plotH = cssH - padT - padB;
		if (plotW <= 0 || plotH <= 0) return;

		const { freqs, times, psd, nFreq, nTime } = spec;
		const fMin = freqs[0];
		const fMax = freqs[nFreq - 1];
		const tMin = times[0];
		const tMax = times[nTime - 1];

		// Render the heatmap into an offscreen ImageData sized to the plot area
		// (in CSS pixels, which we then upscale via dpr).
		const imgW = Math.max(1, Math.floor(plotW));
		const imgH = Math.max(1, Math.floor(plotH));
		const img = ctx.createImageData(imgW, imgH);
		const dbRange = dbMax - dbMin || 1;

		for (let py = 0; py < imgH; py++) {
			// pixel y=0 is top → highest frequency
			const fNorm = 1 - py / (imgH - 1 || 1);
			const fIdxF = fNorm * (nFreq - 1);
			const fIdx = Math.min(nFreq - 1, Math.max(0, Math.round(fIdxF)));
			for (let px = 0; px < imgW; px++) {
				const tNorm = px / (imgW - 1 || 1);
				const tIdxF = tNorm * (nTime - 1);
				const tIdx = Math.min(nTime - 1, Math.max(0, Math.round(tIdxF)));
				const v = psd[fIdx * nTime + tIdx];
				const norm = (v - dbMin) / dbRange;
				const [r, g, b] = viridis(norm);
				const off = (py * imgW + px) * 4;
				img.data[off] = r;
				img.data[off + 1] = g;
				img.data[off + 2] = b;
				img.data[off + 3] = 255;
			}
		}

		// Draw heatmap to a temporary canvas at native imgW×imgH, then drawImage
		// stretched to the plot area (so dpr scaling works correctly).
		const tmp = document.createElement('canvas');
		tmp.width = imgW;
		tmp.height = imgH;
		const tmpCtx = tmp.getContext('2d');
		if (tmpCtx) {
			tmpCtx.putImageData(img, 0, 0);
			ctx.imageSmoothingEnabled = false;
			ctx.drawImage(tmp, padL, padT, plotW, plotH);
		}

		// --- Border ---
		ctx.strokeStyle = '#d1d5db';
		ctx.lineWidth = 1;
		ctx.strokeRect(padL, padT, plotW, plotH);

		// --- Axes (ticks + labels) ---
		ctx.fillStyle = '#9ca3af';
		ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
		ctx.strokeStyle = '#9ca3af';

		// Frequency ticks (Y axis, left side)
		const fTicks = niceTicks(fMin, fMax, Math.max(4, Math.floor(plotH / 50)));
		ctx.textAlign = 'right';
		ctx.textBaseline = 'middle';
		for (const t of fTicks) {
			if (t < fMin || t > fMax) continue;
			const norm = (t - fMin) / (fMax - fMin);
			const y = padT + plotH - norm * plotH;
			ctx.beginPath();
			ctx.moveTo(padL - 3, y);
			ctx.lineTo(padL, y);
			ctx.stroke();
			ctx.fillText(t.toFixed(0), padL - 6, y);
		}

		// Time ticks (X axis, bottom)
		const xTicks = niceTicks(tMin, tMax, Math.max(4, Math.floor(plotW / 80)));
		ctx.textAlign = 'center';
		ctx.textBaseline = 'top';
		for (const t of xTicks) {
			if (t < tMin || t > tMax) continue;
			const norm = (t - tMin) / (tMax - tMin);
			const x = padL + norm * plotW;
			ctx.beginPath();
			ctx.moveTo(x, padT + plotH);
			ctx.lineTo(x, padT + plotH + 3);
			ctx.stroke();
			ctx.fillText(formatTimeTick(t), x, padT + plotH + 6);
		}

		// Axis labels
		ctx.save();
		ctx.translate(14, padT + plotH / 2);
		ctx.rotate(-Math.PI / 2);
		ctx.textAlign = 'center';
		ctx.textBaseline = 'alphabetic';
		ctx.fillText('[Hz]', 0, 0);
		ctx.restore();

		// --- Color bar (right side) ---
		const cbX = cssW - padR + 18;
		const cbW = 12;
		const cbY = padT;
		const cbH = plotH;

		// Build colorbar gradient as ImageData (top = max, bottom = min)
		const cbImg = ctx.createImageData(cbW, cbH);
		for (let py = 0; py < cbH; py++) {
			const norm = 1 - py / (cbH - 1 || 1);
			const [r, g, b] = viridis(norm);
			for (let px = 0; px < cbW; px++) {
				const off = (py * cbW + px) * 4;
				cbImg.data[off] = r;
				cbImg.data[off + 1] = g;
				cbImg.data[off + 2] = b;
				cbImg.data[off + 3] = 255;
			}
		}
		const cbTmp = document.createElement('canvas');
		cbTmp.width = cbW;
		cbTmp.height = cbH;
		const cbTmpCtx = cbTmp.getContext('2d');
		if (cbTmpCtx) {
			cbTmpCtx.putImageData(cbImg, 0, 0);
			ctx.drawImage(cbTmp, cbX, cbY);
		}
		ctx.strokeRect(cbX, cbY, cbW, cbH);

		// Color bar tick labels
		const cbTicks = niceTicks(dbMin, dbMax, 5);
		ctx.textAlign = 'left';
		ctx.textBaseline = 'middle';
		for (const t of cbTicks) {
			if (t < dbMin || t > dbMax) continue;
			const norm = (t - dbMin) / (dbMax - dbMin || 1);
			const y = cbY + cbH - norm * cbH;
			ctx.beginPath();
			ctx.moveTo(cbX + cbW, y);
			ctx.lineTo(cbX + cbW + 3, y);
			ctx.stroke();
			ctx.fillText(t.toFixed(0), cbX + cbW + 6, y);
		}
		ctx.textAlign = 'center';
		ctx.fillText('[dB]', cbX + cbW / 2, cbY + cbH + 14);
	}

	onMount(() => {
		if (containerEl) {
			resizeObserver = new ResizeObserver((entries) => {
				for (const entry of entries) {
					const w = entry.contentRect.width;
					if (w > 0) {
						plotHeight = w < 640 ? 220 : 300;
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
			<span class="text-xs sm:text-sm font-medium text-gray-900">{config.yLabel || 'Acceleration Power Spectral Density'}</span>
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
				<span class="ml-2 text-sm text-gray-400">Computing FFT...</span>
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
