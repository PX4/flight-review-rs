<script lang="ts">
	let {
		totalLogs,
		flightHours,
		uniqueVehicles,
		todayUploads,
		loading,
	}: {
		totalLogs: number;
		flightHours: number;
		uniqueVehicles: number;
		todayUploads: number;
		loading: boolean;
	} = $props();

	function formatHours(h: number): string {
		if (h >= 1000) return `${(h / 1000).toFixed(1)}k`;
		return h.toFixed(1);
	}
</script>

<div class="grid grid-cols-2 gap-3 sm:gap-4 md:grid-cols-4">
	{#if loading}
		{#each Array(4) as _}
			<div class="animate-pulse rounded-lg bg-white p-4 sm:p-6 ring-1 ring-gray-200">
				<div class="h-4 w-24 bg-gray-200 rounded"></div>
				<div class="mt-3 h-8 w-16 bg-gray-200 rounded"></div>
			</div>
		{/each}
	{:else}
		<div class="rounded-lg bg-white p-4 sm:p-6 ring-1 ring-gray-200">
			<dt class="text-sm font-medium text-gray-500">Total Logs</dt>
			<dd class="mt-1 text-3xl font-semibold text-gray-900">{totalLogs.toLocaleString()}</dd>
		</div>
		<div class="rounded-lg bg-white p-4 sm:p-6 ring-1 ring-gray-200">
			<dt class="text-sm font-medium text-gray-500">Flight Hours</dt>
			<dd class="mt-1 text-3xl font-semibold text-gray-900">{formatHours(flightHours)}</dd>
		</div>
		<div class="rounded-lg bg-white p-4 sm:p-6 ring-1 ring-gray-200">
			<dt class="text-sm font-medium text-gray-500">Unique Vehicles</dt>
			<dd class="mt-1 text-3xl font-semibold text-gray-900">{uniqueVehicles.toLocaleString()}</dd>
		</div>
		<div class="rounded-lg bg-white p-4 sm:p-6 ring-1 ring-gray-200">
			<dt class="text-sm font-medium text-gray-500">Today's Uploads</dt>
			<dd class="mt-1 text-3xl font-semibold text-gray-900">{todayUploads.toLocaleString()}</dd>
		</div>
	{/if}
</div>
