<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import type { LogRecord, FlightMetadata } from '$lib/types';
	import { getLog, getMetadata, ApiError } from '$lib/api';
	import LoadingSpinner from '$lib/components/shared/LoadingSpinner.svelte';
	import ErrorBanner from '$lib/components/shared/ErrorBanner.svelte';
	import FlightSummaryHeader from '$lib/components/viewer/FlightSummaryHeader.svelte';
	import FlightModeTimeline from '$lib/components/viewer/FlightModeTimeline.svelte';
	import TopicTreeSidebar from '$lib/components/viewer/TopicTreeSidebar.svelte';
	import PlotContainer from '$lib/components/viewer/PlotContainer.svelte';

	let logRecord = $state<LogRecord | null>(null);
	let metadata = $state<FlightMetadata | null>(null);
	let loading = $state(true);
	let error = $state('');

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
	<div class="flex h-[calc(100vh-4rem)] flex-col overflow-hidden">
		<!-- Flight Summary Header -->
		<FlightSummaryHeader {metadata} logId={logRecord.id} />

		<!-- Flight Mode Timeline -->
		{#if metadata.analysis?.flight_modes && metadata.analysis.flight_modes.length > 0}
			<FlightModeTimeline segments={metadata.analysis.flight_modes} />
		{/if}

		<!-- Main area: sidebar + plots -->
		<div class="flex flex-1 overflow-hidden">
			<!-- Topic Tree Sidebar -->
			<div class="hidden lg:flex lg:w-72 lg:flex-col lg:border-r lg:border-gray-200 bg-white shrink-0">
				<TopicTreeSidebar {metadata} />
			</div>

			<!-- Plot area -->
			<div class="flex-1 flex flex-col overflow-hidden">
				<PlotContainer {metadata} logId={logRecord.id} />
			</div>
		</div>
	</div>
{/if}
