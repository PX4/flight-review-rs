<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { getStats } from '$lib/api';
	import type { StatsFilters, StatsDataPoint } from '$lib/types';
	import StatsFilterPanel from '$lib/components/stats/StatsFilterPanel.svelte';
	import KpiCards from '$lib/components/stats/KpiCards.svelte';
	import HardwareBars from '$lib/components/stats/HardwareBars.svelte';
	import VehicleTypeDonut from '$lib/components/stats/VehicleTypeDonut.svelte';
	import DurationHistogram from '$lib/components/stats/DurationHistogram.svelte';
	import AirframesTable from '$lib/components/stats/AirframesTable.svelte';

	// URL-driven state
	let period = $derived(page.url.searchParams.get('period') ?? 'all');
	let filters = $derived.by((): StatsFilters => ({
		vehicleType: page.url.searchParams.get('vehicle_type') || undefined,
		verHw: page.url.searchParams.get('ver_hw') || undefined,
		source: page.url.searchParams.get('source') || undefined,
	}));

	// Data state
	let hwData = $state<StatsDataPoint[]>([]);
	let vehicleData = $state<StatsDataPoint[]>([]);
	let durationData = $state<StatsDataPoint[]>([]);
	let airframeData = $state<StatsDataPoint[]>([]);

	let totalLogs = $state(0);
	let flightHours = $state(0);
	let uniqueVehicles = $state(0);
	let todayUploads = $state(0);

	let loadingHw = $state(true);
	let loadingVehicle = $state(true);
	let loadingDuration = $state(true);
	let loadingAirframe = $state(true);

	let error = $state<string | null>(null);

	function updateUrl(updates: Record<string, string | undefined>) {
		const params = new URLSearchParams(page.url.searchParams);
		for (const [key, value] of Object.entries(updates)) {
			if (value === undefined || value === '') {
				params.delete(key);
			} else {
				params.set(key, value);
			}
		}
		const search = params.toString();
		goto(`/stats${search ? `?${search}` : ''}`, { replaceState: true, keepFocus: true });
	}

	function handlePeriodChange(newPeriod: string) {
		updateUrl({ period: newPeriod === 'all' ? undefined : newPeriod });
	}

	function handleFiltersChange(newFilters: StatsFilters) {
		updateUrl({
			vehicle_type: newFilters.vehicleType,
			ver_hw: newFilters.verHw,
			source: newFilters.source,
		});
	}

	function handleHardwareClick(hardware: string) {
		updateUrl({ ver_hw: hardware });
	}

	function handleAirframeClick(airframe: string) {
		updateUrl({ vehicle_type: airframe });
	}

	function buildParams(groupBy: string) {
		return {
			group_by: groupBy,
			period: period === 'all' ? undefined : period,
			vehicle_type: filters.vehicleType,
			ver_hw: filters.verHw,
			source: filters.source,
		};
	}

	// Fetch all stats data in parallel when filters change
	$effect(() => {
		const _period = period;
		const _filters = filters;

		error = null;
		loadingHw = true;
		loadingVehicle = true;
		loadingDuration = true;
		loadingAirframe = true;

		// Hardware stats
		getStats({ ...buildParams('ver_hw'), limit: 15 })
			.then((res) => {
				hwData = res.data;
				// Compute KPIs from hardware data (total across all groups)
				totalLogs = res.data.reduce((s, d) => s + d.count, 0);
				flightHours = res.data.reduce((s, d) => s + (d.total_flight_hours ?? 0), 0);
				uniqueVehicles = res.data.length;
				loadingHw = false;
			})
			.catch((err: Error) => {
				error = err.message || 'Failed to load hardware stats';
				loadingHw = false;
			});

		// Vehicle type stats
		getStats(buildParams('vehicle_type'))
			.then((res) => {
				vehicleData = res.data;
				loadingVehicle = false;
			})
			.catch((err: Error) => {
				error = err.message || 'Failed to load vehicle type stats';
				loadingVehicle = false;
			});

		// Duration distribution — use mission_type as a proxy since duration_bucket isn't a valid group_by
		getStats(buildParams('mission_type'))
			.then((res) => {
				durationData = res.data;
				loadingDuration = false;
			})
			.catch((err: Error) => {
				// Non-critical, just hide the chart
				durationData = [];
				loadingDuration = false;
			});

		// Airframe stats
		getStats({ ...buildParams('sys_name'), limit: 25 })
			.then((res) => {
				airframeData = res.data;
				// Today's uploads from a separate field if available
				todayUploads = res.data.length > 0 ? res.data.reduce((s, d) => s + d.count, 0) : 0;
				loadingAirframe = false;
			})
			.catch((err: Error) => {
				error = err.message || 'Failed to load airframe stats';
				loadingAirframe = false;
			});
	});
</script>

<svelte:head>
	<title>Statistics - Flight Review</title>
</svelte:head>

<div class="px-4 sm:px-6 lg:px-8 py-8">
	<!-- Header -->
	<div class="mb-6">
		<h1 class="text-base font-semibold text-gray-900 dark:text-gray-100">Statistics</h1>
		<p class="mt-1 text-sm text-gray-500 dark:text-gray-400">Flight log analytics and trends</p>
	</div>

	<!-- Filter panel -->
	<div class="mb-6">
		<StatsFilterPanel
			{period}
			{filters}
			onPeriodChange={handlePeriodChange}
			onFiltersChange={handleFiltersChange}
		/>
	</div>

	<!-- Error banner -->
	{#if error}
		<div class="rounded-md bg-red-50 p-4 mb-6 dark:bg-red-900/20">
			<div class="flex">
				<div class="shrink-0">
					<svg class="size-5 text-red-400" viewBox="0 0 20 20" fill="currentColor">
						<path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clip-rule="evenodd" />
					</svg>
				</div>
				<div class="ml-3">
					<p class="text-sm text-red-700 dark:text-red-400">{error}</p>
				</div>
			</div>
		</div>
	{/if}

	<!-- KPI Cards -->
	<div class="mb-6">
		<KpiCards
			{totalLogs}
			{flightHours}
			{uniqueVehicles}
			{todayUploads}
			loading={loadingHw}
		/>
	</div>

	<!-- Charts grid: single column on mobile, 2 columns on lg+ -->
	<div class="grid grid-cols-1 md:grid-cols-2 gap-6">
		<HardwareBars data={hwData} loading={loadingHw} onBarClick={handleHardwareClick} />
		<VehicleTypeDonut data={vehicleData} loading={loadingVehicle} />
		<DurationHistogram data={durationData} loading={loadingDuration} />
		<AirframesTable data={airframeData} loading={loadingAirframe} onAirframeClick={handleAirframeClick} />
	</div>
</div>
