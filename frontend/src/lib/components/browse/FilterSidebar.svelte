<script lang="ts">
	import type { ListFilters } from '$lib/types';

	let { filters, onChange }: {
		filters: ListFilters;
		onChange: (filters: Partial<ListFilters>) => void;
	} = $props();

	let searchValue = $state('');
	let vehicleValue = $state('');
	let hardwareValue = $state('');

	let searchTimer: ReturnType<typeof setTimeout> | undefined;
	let vehicleTimer: ReturnType<typeof setTimeout> | undefined;
	let hardwareTimer: ReturnType<typeof setTimeout> | undefined;

	function debounce(
		value: string,
		field: keyof ListFilters,
		timer: ReturnType<typeof setTimeout> | undefined,
	): ReturnType<typeof setTimeout> {
		if (timer) clearTimeout(timer);
		return setTimeout(() => {
			onChange({ [field]: value || undefined });
		}, 300);
	}

	function handleSearch(e: Event) {
		searchValue = (e.target as HTMLInputElement).value;
		searchTimer = debounce(searchValue, 'search', searchTimer);
	}

	function handleVehicle(e: Event) {
		vehicleValue = (e.target as HTMLInputElement).value;
		vehicleTimer = debounce(vehicleValue, 'sys_name', vehicleTimer);
	}

	function handleHardware(e: Event) {
		hardwareValue = (e.target as HTMLInputElement).value;
		hardwareTimer = debounce(hardwareValue, 'ver_hw', hardwareTimer);
	}

	function clearAll() {
		searchValue = '';
		vehicleValue = '';
		hardwareValue = '';
		if (searchTimer) clearTimeout(searchTimer);
		if (vehicleTimer) clearTimeout(vehicleTimer);
		if (hardwareTimer) clearTimeout(hardwareTimer);
		onChange({ search: undefined, sys_name: undefined, ver_hw: undefined });
	}

	// Sync local state when filters prop changes externally
	$effect(() => {
		searchValue = filters.search ?? '';
		vehicleValue = filters.sys_name ?? '';
		hardwareValue = filters.ver_hw ?? '';
	});
</script>

<div>
	<div class="text-xs/6 font-semibold text-gray-500">Filters</div>
	<div class="mt-3 space-y-3">
		<div>
			<label for="filter-search" class="block text-xs font-medium text-gray-500 mb-1">Search</label>
			<input
				id="filter-search"
				type="text"
				placeholder="Search logs..."
				value={searchValue}
				oninput={handleSearch}
				class="block w-full rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
			/>
		</div>
		<div>
			<label for="filter-vehicle" class="block text-xs font-medium text-gray-500 mb-1">Vehicle</label>
			<input
				id="filter-vehicle"
				type="text"
				placeholder="e.g. PX4 Autopilot"
				value={vehicleValue}
				oninput={handleVehicle}
				class="block w-full rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
			/>
		</div>
		<div>
			<label for="filter-hardware" class="block text-xs font-medium text-gray-500 mb-1">Hardware</label>
			<input
				id="filter-hardware"
				type="text"
				placeholder="e.g. Pixhawk 6C"
				value={hardwareValue}
				oninput={handleHardware}
				class="block w-full rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
			/>
		</div>
		<button
			type="button"
			onclick={clearAll}
			class="text-xs text-indigo-600 hover:text-indigo-500"
		>
			Clear all filters
		</button>
	</div>
</div>
