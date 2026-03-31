<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { FlightMetadata } from '$lib/types';
	import { activePlots, activePanel } from '$lib/stores/logViewer';
	import { timeRange } from '$lib/stores/plotSync';
	import { terminateDuckDB } from '$lib/utils/duckdb';
	import PlotStrip from './PlotStrip.svelte';
	import MapPanel from './MapPanel.svelte';
	import LogMessagesPanel from './LogMessagesPanel.svelte';
	import ParamDiffPanel from './ParamDiffPanel.svelte';

	let { metadata, logId } = $props<{ metadata: FlightMetadata; logId: string }>();

	onDestroy(() => {
		terminateDuckDB();
	});

	const tabs = [
		{ id: 'plots' as const, label: 'Plots' },
		{ id: 'map' as const, label: 'Map' },
		{ id: 'messages' as const, label: 'Messages' },
		{ id: 'params' as const, label: 'Parameters' },
	];

	function resetZoom() {
		timeRange.set(null);
	}

	let flightStartUs = $derived.by(() => {
		const modes = metadata.analysis?.flight_modes;
		if (modes && modes.length > 0) return modes[0].start_us;
		return 0;
	});
</script>

<!-- Tab bar -->
<div class="border-b border-gray-200 px-4 overflow-x-auto">
	<div class="flex gap-4 min-w-max">
		{#each tabs as tab}
			<button
				class="border-b-2 px-1 py-2.5 text-sm font-medium {$activePanel === tab.id
					? 'border-indigo-500 text-indigo-600'
					: 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'}"
				onclick={() => activePanel.set(tab.id)}
			>
				{tab.label}
			</button>
		{/each}
		<div class="ml-auto flex items-center">
			{#if $activePanel === 'plots'}
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

<!-- Panel Area -->
<div class="flex-1 lg:overflow-y-auto p-4 space-y-4">
	{#if $activePanel === 'plots'}
		{#if $activePlots.length > 0}
			{#each $activePlots as plot (plot.id)}
				<PlotStrip config={plot} {logId} {metadata} />
			{/each}
		{:else}
			<div class="flex flex-col items-center justify-center py-24 text-center">
				<svg class="size-12 text-gray-300 mb-4" fill="none" viewBox="0 0 24 24" stroke-width="1" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z" />
				</svg>
				<h3 class="text-sm font-medium text-gray-900">No plots selected</h3>
				<p class="mt-1 text-sm text-gray-500">Expand a topic in the sidebar and select fields to plot.</p>
			</div>
		{/if}
	{:else if $activePanel === 'map'}
		<MapPanel
			track={metadata.analysis?.gps_track ?? []}
			modes={metadata.analysis?.flight_modes ?? []}
		/>
	{:else if $activePanel === 'messages'}
		<LogMessagesPanel
			messages={metadata.logged_messages}
			{flightStartUs}
		/>
	{:else if $activePanel === 'params'}
		<ParamDiffPanel
			diffs={metadata.analysis?.non_default_params ?? []}
			changedParams={metadata.changed_parameters}
			allParameters={metadata.parameters}
			defaultParameters={metadata.default_parameters}
		/>
	{/if}
</div>
