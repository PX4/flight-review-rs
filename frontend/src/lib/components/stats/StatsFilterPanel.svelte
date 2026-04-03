<script lang="ts">
	import type { StatsFilters } from '$lib/types';

	let {
		period,
		filters,
		onPeriodChange,
		onFiltersChange,
	}: {
		period: string;
		filters: StatsFilters;
		onPeriodChange: (period: string) => void;
		onFiltersChange: (filters: StatsFilters) => void;
	} = $props();

	const periods = [
		{ value: '7d', label: '7d' },
		{ value: '30d', label: '30d' },
		{ value: '90d', label: '90d' },
		{ value: '1y', label: '1y' },
		{ value: 'all', label: 'All' },
	];

	let vehicleType = $state('');
	let verHw = $state('');
	let source = $state('');

	function applyFilters() {
		onFiltersChange({
			vehicleType: vehicleType || undefined,
			verHw: verHw || undefined,
			source: source || undefined,
		});
	}

	function clearAll() {
		vehicleType = '';
		verHw = '';
		source = '';
		onPeriodChange('all');
		onFiltersChange({});
	}

	$effect(() => {
		vehicleType = filters.vehicleType ?? '';
		verHw = filters.verHw ?? '';
		source = filters.source ?? '';
	});
</script>

<div class="flex flex-wrap items-center gap-4">
	<!-- Time range toggles -->
	<div class="flex items-center gap-1 rounded-lg bg-gray-100 p-1">
		{#each periods as p}
			<button
				type="button"
				onclick={() => onPeriodChange(p.value)}
				class="rounded-md px-3 py-1.5 text-sm font-medium transition-colors {period === p.value
					? 'bg-white text-gray-900 shadow-sm'
					: 'text-gray-500 hover:text-gray-700'}"
			>
				{p.label}
			</button>
		{/each}
	</div>

	<!-- Filter dropdowns -->
	<div class="flex flex-wrap items-center gap-2">
		<select
			bind:value={vehicleType}
			onchange={applyFilters}
			class="rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
		>
			<option value="">All Vehicle Types</option>
			<option value="Multirotor">Multirotor</option>
			<option value="Fixed Wing">Fixed Wing</option>
			<option value="VTOL">VTOL</option>
			<option value="Rover">Rover</option>
		</select>

		<input
			type="text"
			placeholder="Hardware..."
			bind:value={verHw}
			onchange={applyFilters}
			class="w-36 rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
		/>

		<input
			type="text"
			placeholder="Source..."
			bind:value={source}
			onchange={applyFilters}
			class="w-28 rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
		/>
	</div>

	<!-- Clear all -->
	{#if period !== 'all' || filters.vehicleType || filters.verHw || filters.source}
		<button
			type="button"
			onclick={clearAll}
			class="text-xs text-indigo-600 hover:text-indigo-500"
		>
			Clear all
		</button>
	{/if}
</div>
