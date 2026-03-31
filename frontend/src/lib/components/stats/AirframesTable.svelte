<script lang="ts">
	import type { StatsDataPoint } from '$lib/types';

	let {
		data,
		loading,
		onAirframeClick,
	}: {
		data: StatsDataPoint[];
		loading: boolean;
		onAirframeClick?: (airframe: string) => void;
	} = $props();

	type SortKey = 'group' | 'count' | 'total_flight_hours' | 'avg_flight_duration_s';
	let sortKey = $state<SortKey>('count');
	let sortDir = $state<'asc' | 'desc'>('desc');

	function handleSort(key: SortKey) {
		if (sortKey === key) {
			sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		} else {
			sortKey = key;
			sortDir = 'desc';
		}
	}

	let sorted = $derived.by(() => {
		const items = [...data];
		items.sort((a, b) => {
			let av: number | string;
			let bv: number | string;
			if (sortKey === 'group') {
				av = a.group.toLowerCase();
				bv = b.group.toLowerCase();
			} else if (sortKey === 'count') {
				av = a.count;
				bv = b.count;
			} else if (sortKey === 'total_flight_hours') {
				av = a.total_flight_hours ?? 0;
				bv = b.total_flight_hours ?? 0;
			} else {
				av = a.avg_flight_duration_s ?? 0;
				bv = b.avg_flight_duration_s ?? 0;
			}
			if (av < bv) return sortDir === 'asc' ? -1 : 1;
			if (av > bv) return sortDir === 'asc' ? 1 : -1;
			return 0;
		});
		return items;
	});

	function formatDuration(seconds: number | undefined): string {
		if (seconds == null) return '-';
		if (seconds < 60) return `${seconds.toFixed(0)}s`;
		if (seconds < 3600) return `${(seconds / 60).toFixed(1)}m`;
		return `${(seconds / 3600).toFixed(1)}h`;
	}

	function formatHours(hours: number | undefined): string {
		if (hours == null) return '-';
		return `${hours.toFixed(1)}h`;
	}

	function sortIndicator(key: SortKey): string {
		if (sortKey !== key) return '';
		return sortDir === 'asc' ? ' \u2191' : ' \u2193';
	}
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 overflow-hidden">
	<div class="px-6 py-4">
		<h3 class="text-sm font-medium text-gray-500">Airframes</h3>
	</div>
	{#if loading}
		<div class="animate-pulse px-6 pb-6 space-y-3">
			{#each Array(6) as _}
				<div class="h-6 bg-gray-100 rounded w-full"></div>
			{/each}
		</div>
	{:else if data.length === 0}
		<p class="text-sm text-gray-400 py-8 text-center">No data available</p>
	{:else}
		<div class="overflow-x-auto">
			<table class="min-w-full divide-y divide-gray-200">
				<thead class="bg-gray-50">
					<tr>
						<th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider w-12">#</th>
						<th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider cursor-pointer select-none hover:text-gray-700" onclick={() => handleSort('group')}>
							Airframe{sortIndicator('group')}
						</th>
						<th class="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider cursor-pointer select-none hover:text-gray-700" onclick={() => handleSort('count')}>
							Flights{sortIndicator('count')}
						</th>
						<th class="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider cursor-pointer select-none hover:text-gray-700" onclick={() => handleSort('total_flight_hours')}>
							Total Hours{sortIndicator('total_flight_hours')}
						</th>
						<th class="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider cursor-pointer select-none hover:text-gray-700" onclick={() => handleSort('avg_flight_duration_s')}>
							Avg Duration{sortIndicator('avg_flight_duration_s')}
						</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-gray-100">
					{#each sorted as item, i}
						<tr class="hover:bg-gray-50">
							<td class="px-6 py-3 text-sm text-gray-400">{i + 1}</td>
							<td class="px-6 py-3 text-sm text-gray-900">
								{#if onAirframeClick}
									<button
										type="button"
										class="text-indigo-600 hover:text-indigo-500 hover:underline"
										onclick={() => onAirframeClick(item.group)}
									>
										{item.group}
									</button>
								{:else}
									{item.group}
								{/if}
							</td>
							<td class="px-6 py-3 text-sm text-gray-700 text-right">{item.count.toLocaleString()}</td>
							<td class="px-6 py-3 text-sm text-gray-700 text-right">{formatHours(item.total_flight_hours)}</td>
							<td class="px-6 py-3 text-sm text-gray-700 text-right">{formatDuration(item.avg_flight_duration_s)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
