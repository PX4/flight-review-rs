<script lang="ts">
	import { getContext } from 'svelte';
	import type { FlightMetadata } from '$lib/types';
	import { activePlots } from '$lib/stores/logViewer';
	import PlotStrip from '$lib/components/viewer/PlotStrip.svelte';

	const ctx = getContext<{ metadata: FlightMetadata; logId: string }>('log-viewer');
</script>

{#if $activePlots.length > 0}
	{#each $activePlots as plot (plot.id)}
		<PlotStrip config={plot} logId={ctx.logId} metadata={ctx.metadata} />
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
