<script lang="ts">
	import { Chart, DoughnutController, ArcElement, Tooltip, Legend } from 'chart.js';
	import type { StatsDataPoint } from '$lib/types';

	Chart.register(DoughnutController, ArcElement, Tooltip, Legend);

	let {
		data,
		loading,
	}: {
		data: StatsDataPoint[];
		loading: boolean;
	} = $props();

	const colorMap: Record<string, string> = {
		'Multirotor': '#3B82F6',
		'Fixed Wing': '#10B981',
		'VTOL': '#8B5CF6',
		'Rover': '#F97316',
	};
	const defaultColor = '#9CA3AF';

	let canvasEl: HTMLCanvasElement | undefined = $state();
	let chart: Chart | undefined;

	$effect(() => {
		if (!canvasEl || loading || data.length === 0) return;

		const sorted = [...data].sort((a, b) => b.count - a.count);
		const labels = sorted.map((d) => d.group);
		const counts = sorted.map((d) => d.count);
		const colors = sorted.map((d) => colorMap[d.group] ?? defaultColor);

		if (chart) chart.destroy();

		chart = new Chart(canvasEl, {
			type: 'doughnut',
			data: {
				labels,
				datasets: [
					{
						data: counts,
						backgroundColor: colors,
						borderWidth: 0,
						hoverOffset: 4,
					},
				],
			},
			options: {
				responsive: true,
				maintainAspectRatio: false,
				cutout: '60%',
				plugins: {
					legend: {
						position: 'bottom',
						labels: {
							padding: 16,
							usePointStyle: true,
							pointStyleWidth: 10,
						},
					},
					tooltip: {
						callbacks: {
							label: (ctx) => {
								const total = counts.reduce((s, c) => s + c, 0);
								const pct = total > 0 ? ((ctx.parsed / total) * 100).toFixed(1) : '0';
								return `${ctx.label}: ${ctx.parsed.toLocaleString()} (${pct}%)`;
							},
						},
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
	<h3 class="text-sm font-medium text-gray-500 mb-4">Vehicle Types</h3>
	{#if loading}
		<div class="flex justify-center py-8">
			<div class="animate-pulse h-48 w-48 rounded-full bg-gray-100"></div>
		</div>
	{:else if data.length === 0}
		<p class="text-sm text-gray-400 py-8 text-center">No data available</p>
	{:else}
		<div style="height: 280px">
			<canvas bind:this={canvasEl}></canvas>
		</div>
	{/if}
</div>
