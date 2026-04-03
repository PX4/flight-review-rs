<script lang="ts">
	import { Chart, BarController, BarElement, CategoryScale, LinearScale, Tooltip } from 'chart.js';
	import type { StatsDataPoint } from '$lib/types';
	import { getHardwareName } from '$lib/utils/hardwareNames';

	Chart.register(BarController, BarElement, CategoryScale, LinearScale, Tooltip);

	let {
		data,
		loading,
		onBarClick,
	}: {
		data: StatsDataPoint[];
		loading: boolean;
		onBarClick?: (hardware: string) => void;
	} = $props();

	let canvasEl: HTMLCanvasElement | undefined = $state();
	let chart: Chart | undefined;

	$effect(() => {
		if (!canvasEl || loading || data.length === 0) return;

		const sorted = [...data].sort((a, b) => b.count - a.count).slice(0, 15);
		const labels = sorted.map((d) => getHardwareName(d.group));
		const counts = sorted.map((d) => d.count);

		if (chart) chart.destroy();

		chart = new Chart(canvasEl, {
			type: 'bar',
			data: {
				labels,
				datasets: [
					{
						data: counts,
						backgroundColor: '#6366F1',
						borderRadius: 4,
					},
				],
			},
			options: {
				indexAxis: 'y',
				responsive: true,
				maintainAspectRatio: false,
				plugins: {
					tooltip: {
						callbacks: {
							label: (ctx) => `${(ctx.parsed.x ?? 0).toLocaleString()} logs`,
						},
					},
				},
				scales: {
					x: {
						grid: { display: false },
						ticks: { precision: 0 },
					},
					y: {
						grid: { display: false },
					},
				},
				onClick: (_event, elements) => {
					if (elements.length > 0 && onBarClick) {
						const idx = elements[0].index;
						onBarClick(labels[idx]);
					}
				},
			},
		});

		return () => {
			if (chart) {
				chart.destroy();
				chart = undefined;
			}
		};
	});
</script>

<div class="rounded-lg bg-white p-6 ring-1 ring-gray-200">
	<h3 class="text-sm font-medium text-gray-500 mb-4">Top Hardware Platforms</h3>
	{#if loading}
		<div class="animate-pulse space-y-2">
			{#each Array(8) as _}
				<div class="h-5 bg-gray-100 rounded" style="width: {60 + Math.random() * 40}%"></div>
			{/each}
		</div>
	{:else if data.length === 0}
		<p class="text-sm text-gray-400 py-8 text-center">No data available</p>
	{:else}
		<div style="height: {Math.min(data.length, 15) * 28 + 40}px">
			<canvas bind:this={canvasEl}></canvas>
		</div>
	{/if}
</div>
