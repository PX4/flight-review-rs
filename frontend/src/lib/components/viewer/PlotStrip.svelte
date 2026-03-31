<script lang="ts">
	import type { PlotConfig, FlightMetadata } from '$lib/types';
	import { activePlots } from '$lib/stores/logViewer';

	let { config, logId, metadata } = $props<{
		config: PlotConfig;
		logId: string;
		metadata: FlightMetadata;
	}>();

	function removePlot() {
		activePlots.update((plots) => plots.filter((p) => p.id !== config.id));
	}
</script>

<div class="rounded-lg ring-1 ring-gray-200 bg-white overflow-hidden">
	<div class="flex items-center justify-between px-4 py-2.5 border-b border-gray-100">
		<div class="flex items-center gap-4">
			<span class="text-sm font-medium text-gray-900">{config.topic}</span>
			<div class="flex items-center gap-3 text-xs">
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
	<div class="relative h-48 bg-gray-50 flex items-center justify-center">
		<!-- MVP placeholder - real uPlot integration in follow-up -->
		<svg class="w-3/4 h-3/4 opacity-20" viewBox="0 0 800 180" preserveAspectRatio="none">
			<path
				d="M40 90 C80 50, 120 130, 160 80 C200 40, 240 110, 280 70 C320 40, 360 120, 400 85 C440 60, 480 100, 520 75 C560 50, 600 110, 640 80 C680 55, 720 95, 760 70"
				fill="none"
				stroke="#9ca3af"
				stroke-width="1.5"
			/>
		</svg>
		<p class="absolute text-sm text-gray-400">Data visualization coming soon</p>
	</div>
</div>
