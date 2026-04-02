<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import uPlot from 'uplot';
	import type { PlotConfig, FlightMetadata } from '$lib/types';
	import { activePlots } from '$lib/stores/logViewer';
	import { timeRange, cursorTimestamp, SYNC_KEY } from '$lib/stores/plotSync';
	import { initDuckDB, LogSession } from '$lib/utils/duckdb';

	let { config, logId, metadata } = $props<{
		config: PlotConfig;
		logId: string;
		metadata: FlightMetadata;
	}>();

	// Module-level session cache (shared across all PlotStrip instances)
	const sessionCache = (globalThis as any).__plotSessionCache ??= new Map<string, LogSession>();

	let containerEl: HTMLDivElement | undefined = $state();
	let chartEl: HTMLDivElement | undefined = $state();
	let uplot: uPlot | null = null;
	let resizeObserver: ResizeObserver | null = null;

	let loading = $state(true);
	let error = $state<string | null>(null);
	let plotHeight = $state(200);

	// Guard against infinite loops when syncing scales
	let settingScale = false;

	// Track the fields key to detect changes
	let lastFieldsKey = '';

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

	async function loadAndRender(fields: string[], colors: string[]) {
		const fieldsKey = fields.join(',');
		if (fieldsKey === lastFieldsKey && uplot) return;
		lastFieldsKey = fieldsKey;

		loading = true;
		error = null;

		try {
			const session = await getSession();
			const result = await session.queryTopic(config.topic, fields, {
				multiId: config.multiId
			});

			if (!result) {
				error = 'No data returned';
				loading = false;
				return;
			}

			const data: uPlot.AlignedData = [result.timestamps, ...result.series];

			const containerWidth = containerEl?.clientWidth ?? 800;
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
				cursor: {
					sync: { key: SYNC_KEY, setSeries: true },
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
							if (scaleKey !== 'x' || settingScale) return;
							const min = u.scales.x.min;
							const max = u.scales.x.max;
							if (min != null && max != null) {
								timeRange.set([min, max]);
							}
						},
					],
					setCursor: [
						(u: uPlot) => {
							const idx = u.cursor.idx;
							if (idx != null && data[0]) {
								cursorTimestamp.set(data[0][idx]);
							}
						},
					],
				},
				series: [
					{}, // x-axis series
					...fields.map((field: string, i: number) => ({
						label: field,
						stroke: colors[i] ?? '#818cf8',
						width: 1.5,
					})),
				],
			};

			if (chartEl) {
				uplot = new uPlot(opts, data, chartEl);
			}

			loading = false;
		} catch (e) {
			console.error('PlotStrip load error:', e);
			error = e instanceof Error ? e.message : 'Failed to load data';
			loading = false;
		}
	}

	onMount(() => {
		// Setup resize observer
		if (containerEl) {
			resizeObserver = new ResizeObserver((entries) => {
				for (const entry of entries) {
					const w = entry.contentRect.width;
					if (w > 0) {
						plotHeight = w < 640 ? 140 : 200;
						if (uplot) {
							uplot.setSize({ width: w, height: plotHeight });
						}
					}
				}
			});
			resizeObserver.observe(containerEl);
		}

		// Initial load
		loadAndRender(config.fields, config.colors);
	});

	// React to config.fields changes (when user adds/removes a field in an existing topic)
	$effect(() => {
		const fields = config.fields;
		const colors = config.colors;
		// Only re-render if fields actually changed
		const key = fields.join(',');
		if (key !== lastFieldsKey) {
			loadAndRender(fields, colors);
		}
	});

	// React to timeRange changes from other plots
	$effect(() => {
		const range = $timeRange;
		if (!uplot || settingScale) return;
		settingScale = true;
		if (range) {
			uplot.setScale('x', { min: range[0], max: range[1] });
		} else {
			// Reset zoom: let uPlot auto-fit to full data range
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
	});
</script>

<svelte:head>
	<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uplot@1.6.31/dist/uPlot.min.css" />
</svelte:head>

<div class="rounded-lg ring-1 ring-gray-200 bg-white overflow-hidden" bind:this={containerEl}>
	<div class="flex items-center justify-between px-2 sm:px-4 py-2 sm:py-2.5 border-b border-gray-100">
		<div class="flex flex-wrap items-center gap-2 sm:gap-4">
			<span class="text-xs sm:text-sm font-medium text-gray-900">{config.topic}</span>
			<div class="flex flex-wrap items-center gap-x-1.5 sm:gap-x-3 gap-y-1 text-xs">
				{#each config.fields as field, i}
					<span class="flex items-center gap-1.5">
						<span class="w-3 h-0.5 rounded" style="background-color: {config.colors[i] ?? '#818cf8'};"></span>
						<span class="text-gray-500">{field}</span>
					</span>
				{/each}
			</div>
		</div>
		<button class="text-gray-400 hover:text-gray-600" onclick={removePlot} aria-label="Remove plot">
			<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
			</svg>
		</button>
	</div>
	<div class="relative bg-gray-50" style="min-height: {plotHeight}px;">
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
		<div bind:this={chartEl}></div>
	</div>
</div>
