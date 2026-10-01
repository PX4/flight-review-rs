<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import uPlot from 'uplot';
	import type { PlotConfig, FlightMetadata } from '$lib/types';
	import { activePlots, togglePlotMinimized, editSqlPlot } from '$lib/stores/logViewer';
	import { timeRange, setTimeRange, cursorTimestamp, SYNC_KEY } from '$lib/stores/plotSync';
	import { initDuckDB, LogSession } from '$lib/utils/duckdb';
	import { touchZoomPlugin } from '$lib/utils/uplotTouchZoom';

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

	// Module-level session cache (shared across all PlotStrip instances)
	const sessionCache = (globalThis as any).__plotSessionCache ??= new Map<string, LogSession>();

	let containerEl: HTMLDivElement | undefined = $state();
	let chartEl: HTMLDivElement | undefined = $state();
	let uplot: uPlot | null = null;
	let resizeObserver: ResizeObserver | null = null;

	// The chart's available width, from the element uPlot renders into. clientWidth
	// is post-layout and excludes scrollbars, so the plot never exceeds its column.
	function measuredWidth(): number {
		return chartEl?.clientWidth || containerEl?.clientWidth || 800;
	}

	let loading = $state(true);
	let error = $state<string | null>(null);
	let plotHeight = $state(300);

	// Series metadata actually rendered. For timeseries plots this mirrors
	// config.fields/colors; for SQL plots it comes from the query result.
	const PLOT_COLORS = ['#818cf8', '#fbbf24', '#34d399', '#f87171', '#a78bfa', '#fb923c', '#38bdf8', '#e879f9'];
	let seriesLabels = $state<string[]>([]);
	let seriesColors = $state<string[]>([]);
	// Whether the x-axis is the shared seconds domain (SQL plots: only with a
	// `timestamp` column). Non-time plots stay out of the time-range sync.
	let timeSynced = $state(true);

	// Guard against infinite loops when syncing scales
	let settingScale = false;

	// Track whether this plot is visible in the viewport
	let isVisible = $state(true);
	let pendingSyncRange: [number, number] | null = null;
	let intersectionObserver: IntersectionObserver | null = null;

	// Track the render key (fields or SQL text) to detect changes
	let lastRenderKey = '';

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

	function editSql() {
		// Hand the config to the builder, which opens in edit mode.
		editSqlPlot.set(config);
	}

	function downloadPng() {
		if (!chartEl) return;
		const canvas = chartEl.querySelector('canvas');
		if (!canvas) return;
		const link = document.createElement('a');
		link.download = `${(config.topic || config.yLabel || 'plot').replace(/[^a-z0-9_-]+/gi, '_')}.png`;
		link.href = canvas.toDataURL('image/png');
		link.click();
	}

	function renderKey(): string {
		// Include yLabel so renaming a plot rebuilds it and refreshes the y-axis label
		const base = config.kind === 'sql' ? `sql:${config.sql ?? ''}` : config.fields.join(',');
		return `${base}|y:${config.yLabel ?? ''}`;
	}

	async function loadAndRender() {
		const key = renderKey();
		if (key === lastRenderKey && uplot) return;
		lastRenderKey = key;

		loading = true;
		error = null;

		try {
			const session = await getSession();

			let xData: Float64Array;
			let series: Float64Array[];
			let labels: string[];

			if (config.kind === 'sql') {
				const result = await session.querySql(config.sql ?? '');
				if (!result) {
					// Don't auto-remove a user's SQL plot on an empty result — surface it.
					error = 'Query returned no rows.';
					loading = false;
					return;
				}
				xData = result.x;
				series = result.series;
				labels = result.labels;
				timeSynced = result.xIsTime;
			} else {
				const result = await session.queryTopic(config.topic, config.fields, {
					multiId: config.multiId
				});
				if (!result) {
					// Auto-remove plots with no data (e.g. field names don't exist in this log version)
					removePlot();
					return;
				}
				xData = result.timestamps;
				series = result.series;
				labels = config.fields;
				timeSynced = true;
			}

			const colors = labels.map((_, i) => config.colors[i] ?? PLOT_COLORS[i % PLOT_COLORS.length]);
			seriesLabels = labels;
			seriesColors = colors;

			const data: uPlot.AlignedData = [xData, ...series];

			// clientWidth is the post-layout, clip-aware inner width, so the plot
			// is never created wider than the column actually allows.
			const containerWidth = measuredWidth();
			plotHeight = containerWidth < 640 ? 180 : 300;

			// Destroy previous chart if any
			if (uplot) {
				uplot.destroy();
				uplot = null;
			}

			// Clear the chart container
			if (chartEl) {
				chartEl.innerHTML = '';
			}

			const opts: uPlot.Options = {
				width: containerWidth,
				height: plotHeight,
				plugins: [touchZoomPlugin()],
				cursor: {
					// Only cross-sync the cursor among plots sharing the time axis.
					sync: timeSynced ? { key: SYNC_KEY, setSeries: false } : undefined,
					drag: { x: true, y: false, setScale: true },
				},
				scales: {
					x: { time: false },
				},
				axes: [
					{
						stroke: '#9ca3af',
						grid: { stroke: '#e5e7eb' },
					},
					{
						stroke: '#9ca3af',
						grid: { stroke: '#e5e7eb' },
						label: config.yLabel || undefined,
					},
				],
				hooks: {
					setScale: [
						(u: uPlot, scaleKey: string) => {
							// Non-time plots must not write into the shared seconds range.
							if (scaleKey !== 'x' || settingScale || !timeSynced) return;
							const min = u.scales.x.min;
							const max = u.scales.x.max;
							if (min != null && max != null) {
								setTimeRange(min, max);
							}
						},
					],
					setCursor: [
						(u: uPlot) => {
							if (!timeSynced) return; // don't broadcast a non-time x as the shared cursor
							const idx = u.cursor.idx;
							if (idx != null && data[0]) {
								cursorTimestamp.set(data[0][idx]);
							}
						},
					],
				},
				series: [
					{}, // x-axis series
					...labels.map((label: string, i: number) => ({
						label,
						stroke: colors[i],
						width: 1.5,
					})),
				],
			};

			if (chartEl) {
				uplot = new uPlot(opts, data, chartEl);
				// Layout may still be settling when the chart is first created
				// (async data load can resolve before the column reaches its
				// final width). Re-measure on the next frame and correct, so the
				// plot never stays wider than its container.
				requestAnimationFrame(() => {
					if (!uplot) return;
					const w = measuredWidth();
					if (w > 0 && w !== uplot.width) {
						plotHeight = w < 640 ? 180 : 300;
						uplot.setSize({ width: w, height: plotHeight });
					}
				});
			}

			loading = false;
		} catch (e) {
			console.error('PlotStrip load error:', e);
			error = e instanceof Error ? e.message : 'Failed to load data';
			loading = false;
		}
	}

	function applyPendingRange() {
		if (!uplot || !pendingSyncRange) return;
		settingScale = true;
		uplot.setScale('x', { min: pendingSyncRange[0], max: pendingSyncRange[1] });
		settingScale = false;
		pendingSyncRange = null;
	}

	onMount(() => {
		// Track visibility to skip off-screen redraws
		if (containerEl) {
			intersectionObserver = new IntersectionObserver(
				(entries) => {
					for (const entry of entries) {
						isVisible = entry.isIntersecting;
						if (isVisible) {
							applyPendingRange();
						}
					}
				},
				{ rootMargin: '100px' } // slight margin so plots update just before scrolling into view
			);
			intersectionObserver.observe(containerEl);
		}

		// Setup resize observer
		if (containerEl) {
			resizeObserver = new ResizeObserver((entries) => {
				for (const entry of entries) {
					const w = entry.contentRect.width;
					if (w > 0) {
						plotHeight = w < 640 ? 180 : 300;
						if (uplot) {
							uplot.setSize({ width: w, height: plotHeight });
						}
					}
				}
			});
			resizeObserver.observe(containerEl);
		}

		// Initial load
		loadAndRender();
	});

	// Re-render when the plot's definition changes: fields for timeseries
	// plots, or the SQL text for SQL plots.
	$effect(() => {
		const key = renderKey();
		if (key !== lastRenderKey) {
			loadAndRender();
		}
	});

	// React to timeRange changes from other plots.
	// Off-screen plots stash the range and apply it when they become visible.
	$effect(() => {
		const range = $timeRange;
		if (!uplot || settingScale) return;
		if (range && !timeSynced) return;

		if (!isVisible) {
			// Stash for later — will be applied when plot scrolls into view
			pendingSyncRange = range ? [range[0], range[1]] : null;
			return;
		}

		settingScale = true;
		if (range) {
			uplot.setScale('x', { min: range[0], max: range[1] });
		} else {
			const data = uplot.data;
			if (data && data[0] && data[0].length > 0) {
				uplot.setScale('x', { min: data[0][0], max: data[0][data[0].length - 1] });
			}
		}
		settingScale = false;
	});

	onDestroy(() => {
		if (uplot) {
			uplot.destroy();
			uplot = null;
		}
		if (resizeObserver) {
			resizeObserver.disconnect();
			resizeObserver = null;
		}
		if (intersectionObserver) {
			intersectionObserver.disconnect();
			intersectionObserver = null;
		}
	});
</script>

<svelte:head>
	<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uplot@1.6.31/dist/uPlot.min.css" />
</svelte:head>

<div class="rounded-lg ring-1 ring-gray-200 bg-white overflow-hidden" bind:this={containerEl}>
	<div class="flex items-center justify-between px-2 sm:px-4 py-2 sm:py-2.5 border-b border-gray-100">
		<div class="flex flex-wrap items-center gap-2 sm:gap-4">
			{#if onDragStart}
				<!-- Drag handle: visible on md+ screens -->
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
				<!-- Mobile reorder buttons: visible below md -->
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
			<span class="text-xs sm:text-sm font-medium text-gray-900">{config.yLabel && config.yLabel !== config.topic ? config.yLabel : config.topic}</span>
			{#if config.kind === 'sql'}
				<span class="text-[10px] font-medium text-indigo-500 bg-indigo-50 rounded px-1.5 py-0.5">SQL</span>
			{:else if config.yLabel && config.yLabel !== config.topic}
				<span class="text-[10px] text-gray-400">{config.topic}</span>
			{/if}
			<div class="flex flex-wrap items-center gap-x-1.5 sm:gap-x-3 gap-y-1 text-xs">
				{#each seriesLabels as label, i}
					<span class="flex items-center gap-1.5">
						<span class="w-3 h-0.5 rounded" style="background-color: {seriesColors[i]};"></span>
						<span class="text-gray-500">{label}</span>
					</span>
				{/each}
			</div>
		</div>
		<div class="flex items-center gap-1">
			{#if config.kind === 'sql'}
				<button class="text-gray-400 hover:text-indigo-600" onclick={editSql} aria-label="Edit SQL">
					<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zM19.5 7.125L16.875 4.5" />
					</svg>
				</button>
			{/if}
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
				<span class="ml-2 text-sm text-gray-400">Loading data...</span>
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
		<div bind:this={chartEl} class="touch-action-none" style="touch-action: none;"></div>
	</div>
</div>
