<script lang="ts">
	import { Chart, BarController, BarElement, CategoryScale, LinearScale, Tooltip } from 'chart.js';
	import type { StatsDataPoint } from '$lib/types';

	Chart.register(BarController, BarElement, CategoryScale, LinearScale, Tooltip);

	let {
		data,
		loading,
	}: {
		data: StatsDataPoint[];
		loading: boolean;
	} = $props();

	const bucketOrder = ['< 1m', '1-5m', '5-15m', '15-30m', '30-60m', '60m+'];

	let canvasEl: HTMLCanvasElement | undefined = $state();
	let chart: Chart | undefined;

	$effect(() => {
		if (!canvasEl || loading || data.length === 0) return;

		// Order buckets according to defined order, fill missing with 0
		const dataMap = new Map(data.map((d) => [d.group, d.count]));
		const labels = bucketOrder;
		const counts = labels.map((l) => dataMap.get(l) ?? 0);

		const barColors = [
			'#DBEAFE', '#93C5FD', '#60A5FA', '#3B82F6', '#2563EB', '#1D4ED8',
		];

		if (chart) chart.destroy();

		chart = new Chart(canvasEl, {
			type: 'bar',
			data: {
				labels,
				datasets: [
					{
						data: counts,
						backgroundColor: barColors,
						borderRadius: 4,
					},
				],
			},
			options: {
				responsive: true,
				maintainAspectRatio: false,
				plugins: {
					tooltip: {
						callbacks: {
							label: (ctx) => `${(ctx.parsed.y ?? 0).toLocaleString()} flights`,
						},
					},
				},
				scales: {
					x: {
						grid: { display: false },
					},
					y: {
						grid: { color: '#F3F4F6' },
						ticks: { precision: 0 },
					},
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
	<h3 class="text-sm font-medium text-gray-500 mb-4">Flight Duration Distribution</h3>
	{#if loading}
		<div class="animate-pulse flex items-end gap-2 h-48 pt-4">
			{#each Array(6) as _, i}
				<div class="flex-1 bg-gray-100 rounded" style="height: {20 + Math.random() * 80}%"></div>
			{/each}
		</div>
	{:else if data.length === 0}
		<p class="text-sm text-gray-400 py-8 text-center">No data available</p>
	{:else}
		<div class="h-[180px] sm:h-[240px]">
			<canvas bind:this={canvasEl}></canvas>
		</div>
	{/if}
</div>
