<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { onMount, onDestroy, setContext } from 'svelte';
	import type { LogRecord, FlightMetadata } from '$lib/types';
	import { getLog, getMetadata, ApiError } from '$lib/api';
	import { terminateDuckDB } from '$lib/utils/duckdb';
	import { timeRange } from '$lib/stores/plotSync';
	import { builderOpen } from '$lib/stores/logViewer';
	import LoadingSpinner from '$lib/components/shared/LoadingSpinner.svelte';
	import ErrorBanner from '$lib/components/shared/ErrorBanner.svelte';
	import FlightSummaryHeader from '$lib/components/viewer/FlightSummaryHeader.svelte';
	import FlightModeTimeline from '$lib/components/viewer/FlightModeTimeline.svelte';
	import PlotBuilderPanel from '$lib/components/viewer/PlotBuilderPanel.svelte';

	let { children } = $props<{ children: Snippet }>();

	let logRecord = $state<LogRecord | null>(null);
	let metadata = $state<FlightMetadata | null>(null);
	let loading = $state(true);
	let error = $state('');

	// Make metadata available to child routes via context
	setContext('log-viewer', {
		get metadata() { return metadata; },
		get logRecord() { return logRecord; },
		get logId() { return page.params.id; },
	});

	async function loadData() {
		loading = true;
		error = '';
		try {
			const id = page.params.id!;
			const [log, meta] = await Promise.all([getLog(id), getMetadata(id)]);
			logRecord = log;
			metadata = meta;
		} catch (e) {
			if (e instanceof ApiError) {
				error = e.status === 404 ? 'Log not found.' : `Error: ${e.message}`;
			} else {
				error = 'Failed to load log data.';
			}
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadData();
	});

	onDestroy(() => {
		// Clear the per-log session cache so closed LogSessions don't linger
		const cache: Map<string, any> | undefined = (globalThis as any).__plotSessionCache;
		if (cache) {
			for (const session of cache.values()) session.close();
			cache.clear();
		}
		terminateDuckDB();
	});

	// Tab routing
	const logBase = $derived(`/log/${page.params.id}`);
	const hasGps = $derived(
		metadata?.analysis?.gps_track != null && metadata.analysis.gps_track.length > 1
	);
	const hasSystemInfo = $derived.by(() => {
		const mi = metadata?.multi_info;
		if (mi == null) return false;
		return ['boot_console_output', 'perf_top_preflight', 'perf_top_postflight', 'perf_counter_preflight', 'perf_counter_postflight']
			.some(k => (mi[k]?.length ?? 0) > 0);
	});

	const tabs = $derived([
		{ label: 'Plots', href: '' },
		...(hasGps ? [{ label: 'Map', href: '/map' }] : []),
		{ label: 'Messages', href: '/messages' },
		{ label: 'Parameters', href: '/parameters' },
		...(hasSystemInfo ? [{ label: 'System', href: '/system' }] : []),
	]);

	let activeTab = $derived.by(() => {
		const path = page.url.pathname;
		const base = `/log/${page.params.id}`;
		const suffix = path.slice(base.length);
		if (suffix === '/map') return '/map';
		if (suffix === '/messages') return '/messages';
		if (suffix === '/parameters') return '/parameters';
		if (suffix === '/system') return '/system';
		return '';
	});

	function resetZoom() {
		timeRange.set(null);
	}
</script>

<svelte:head>
	<title>{metadata?.sys_name ?? 'Log'} - Flight Review</title>
</svelte:head>

{#if loading}
	<div class="flex items-center justify-center h-96">
		<LoadingSpinner />
	</div>
{:else if error}
	<div class="px-4 py-10 sm:px-6 lg:px-8">
		<ErrorBanner message={error} onRetry={loadData} />
	</div>
{:else if metadata && logRecord}
	<div class="flex flex-col {activeTab === '/map' ? 'h-dvh overflow-hidden' : 'min-h-dvh'}">
		<!-- Compact top bar -->
		<div class="flex items-center gap-4 border-b border-gray-200 bg-white px-4 py-2 shrink-0 dark:border-gray-700 dark:bg-gray-900">
			<a href="/browse" class="flex items-center gap-2 text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200">
				<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
				</svg>
				Back to logs
			</a>
			<div class="flex-1"></div>
			<img src="/flight-review-logo.svg" alt="Flight Review" class="h-7 w-auto opacity-60" />
		</div>

		<!-- Sticky header: summary + flight modes + tab bar -->
		<div class="sticky top-0 z-30 bg-white shadow-sm">
			<FlightSummaryHeader {metadata} logId={logRecord.id} vehicleType={logRecord.vehicle_type} locationName={logRecord.location_name} filename={logRecord.filename} createdAt={logRecord.created_at} />

			{#if metadata.analysis?.flight_modes && metadata.analysis.flight_modes.length > 0}
				<FlightModeTimeline segments={metadata.analysis.flight_modes} />
			{/if}

			<!-- Tab bar -->
			<div class="border-b border-gray-200 px-3 sm:px-4 overflow-hidden">
					<div class="flex flex-wrap gap-x-3 sm:gap-x-4">
						{#each tabs as tab}
							<a
								href="{logBase}{tab.href}"
								class="border-b-2 px-0.5 sm:px-1 py-2 sm:py-2.5 text-xs sm:text-sm font-medium {activeTab === tab.href
									? 'border-indigo-500 text-indigo-600'
									: 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'}"
								data-sveltekit-noscroll
							>
								{tab.label}
							</a>
						{/each}
						<div class="ml-auto flex items-center gap-2">
							{#if activeTab === ''}
								<button
									class="flex items-center gap-1.5 rounded-md bg-indigo-600 px-2 sm:px-3 py-1 text-xs font-medium text-white hover:bg-indigo-500"
									onclick={() => builderOpen.set(true)}
								>
									<svg class="size-3.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
										<path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 010 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 010-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28z" />
										<path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
									</svg>
									<span class="hidden sm:inline">Edit Plots</span>
								</button>
								<button
									class="flex items-center gap-1.5 rounded-md bg-white px-2 sm:px-3 py-1 text-xs font-medium text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50"
									onclick={resetZoom}
								>
									<svg class="size-3.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
										<path stroke-linecap="round" stroke-linejoin="round" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25" />
									</svg>
									<span class="hidden sm:inline">Reset Zoom</span>
								</button>
							{/if}
						</div>
					</div>
				</div>
		</div>

		<!-- Main area: content -->
		<div class="flex flex-1 {activeTab === '/map' ? 'min-h-0 overflow-hidden' : ''}">
			<div class="flex-1 flex flex-col {activeTab === '/map' ? 'min-h-0 overflow-hidden' : ''}">
				<!-- Panel content (from child route) -->
				<div class="{activeTab === '/map' ? 'flex-1 flex flex-col min-h-0' : ''} p-3 sm:p-4 space-y-4 overflow-x-hidden">
					{@render children()}
				</div>
			</div>
		</div>
	</div>

	<!-- Plot Builder slide-over panel -->
	<PlotBuilderPanel {metadata} />
{/if}
