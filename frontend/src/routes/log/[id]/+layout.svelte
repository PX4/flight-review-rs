<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { onMount, onDestroy, setContext } from 'svelte';
	import type { LogRecord, FlightMetadata } from '$lib/types';
	import { getLog, getMetadata, ApiError } from '$lib/api';
	import { terminateDuckDB } from '$lib/utils/duckdb';
	import { timeRange } from '$lib/stores/plotSync';
	import LoadingSpinner from '$lib/components/shared/LoadingSpinner.svelte';
	import ErrorBanner from '$lib/components/shared/ErrorBanner.svelte';
	import FlightSummaryHeader from '$lib/components/viewer/FlightSummaryHeader.svelte';
	import FlightModeTimeline from '$lib/components/viewer/FlightModeTimeline.svelte';
	import TopicTreeSidebar from '$lib/components/viewer/TopicTreeSidebar.svelte';

	let { children } = $props<{ children: Snippet }>();

	let logRecord = $state<LogRecord | null>(null);
	let metadata = $state<FlightMetadata | null>(null);
	let loading = $state(true);
	let error = $state('');
	let mobileTopicsOpen = $state(false);

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
		terminateDuckDB();
	});

	// Tab routing
	const logBase = $derived(`/log/${page.params.id}`);
	const tabs = [
		{ label: 'Plots', href: '' },
		{ label: 'Map', href: '/map' },
		{ label: 'Messages', href: '/messages' },
		{ label: 'Parameters', href: '/parameters' },
	];

	let activeTab = $derived.by(() => {
		const path = page.url.pathname;
		const base = `/log/${page.params.id}`;
		const suffix = path.slice(base.length);
		if (suffix === '/map') return '/map';
		if (suffix === '/messages') return '/messages';
		if (suffix === '/parameters') return '/parameters';
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
	<div class="flex h-screen flex-col overflow-x-hidden overflow-y-auto lg:overflow-hidden">
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

		<!-- Flight Summary Header -->
		<FlightSummaryHeader {metadata} logId={logRecord.id} />

		<!-- Flight Mode Timeline -->
		{#if metadata.analysis?.flight_modes && metadata.analysis.flight_modes.length > 0}
			<FlightModeTimeline segments={metadata.analysis.flight_modes} />
		{/if}

		<!-- Main area: sidebar + content -->
		<div class="flex min-h-[50vh] lg:flex-1 lg:min-h-0 lg:overflow-hidden">
			<!-- Topic Tree Sidebar (desktop) -->
			<div class="hidden md:flex md:w-52 lg:w-64 md:flex-col md:border-r md:border-gray-200 bg-white shrink-0 dark:md:border-gray-700 dark:bg-gray-900">
				<TopicTreeSidebar {metadata} />
			</div>

			<!-- Content area with tabs -->
			<div class="flex-1 flex flex-col lg:overflow-hidden">
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
						<div class="ml-auto flex items-center">
							{#if activeTab === ''}
								<button
									class="rounded-md bg-white px-3 py-1 text-xs font-medium text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50"
									onclick={resetZoom}
								>
									Reset Zoom
								</button>
							{/if}
						</div>
					</div>
				</div>

				<!-- Panel content (from child route) -->
				<div class="flex-1 flex flex-col lg:overflow-y-auto p-3 sm:p-4 space-y-4 overflow-x-hidden">
					{@render children()}
				</div>
			</div>
		</div>
	</div>

	<!-- Floating "Topics" button for mobile -->
	<button
		onclick={() => (mobileTopicsOpen = true)}
		class="fixed bottom-6 right-6 z-40 flex items-center gap-2 rounded-full bg-indigo-600 px-4 py-3 text-sm font-medium text-white shadow-lg hover:bg-indigo-500 md:hidden"
		aria-label="Open topic tree"
	>
		<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
			<path stroke-linecap="round" stroke-linejoin="round" d="M3.75 12h16.5m-16.5 3.75h16.5M3.75 19.5h16.5M5.625 4.5h12.75a1.875 1.875 0 0 1 0 3.75H5.625a1.875 1.875 0 0 1 0-3.75Z" />
		</svg>
		Topics
	</button>

	<!-- Mobile topic sidebar slide-over -->
	{#if mobileTopicsOpen}
		<div class="fixed inset-0 z-50 md:hidden">
			<div
				class="fixed inset-0 bg-gray-900/80"
				onclick={() => (mobileTopicsOpen = false)}
				role="presentation"
			></div>
			<div class="fixed inset-y-0 right-0 z-50 w-full max-w-xs overflow-y-auto bg-white dark:bg-gray-900">
				<div class="flex items-center justify-between border-b border-gray-200 px-4 py-3 dark:border-gray-700">
					<h2 class="text-sm font-semibold text-gray-900 dark:text-gray-100">Topics</h2>
					<button
						type="button"
						class="-m-2 p-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
						aria-label="Close topics"
						onclick={() => (mobileTopicsOpen = false)}
					>
						<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
							<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
						</svg>
					</button>
				</div>
				<TopicTreeSidebar {metadata} />
			</div>
		</div>
	{/if}
{/if}
