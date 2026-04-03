<script lang="ts">
	import { getFilterFacets, type FilterFacets } from '$lib/api';
	import type { ListFilters } from '$lib/types';
	import Combobox from '$lib/components/shared/Combobox.svelte';

	let { filters, onChange }: {
		filters: ListFilters;
		onChange: (filters: Partial<ListFilters>) => void;
	} = $props();

	let open = $state(false);
	let facets = $state<FilterFacets | null>(null);
	let facetsLoaded = $state(false);

	// Local input state for debounced text fields
	let hwValue = $state('');
	let fwValue = $state('');
	let locationValue = $state('');
	let includePreReleases = $state(false);

	// Filter firmware versions: stable only by default, all if checkbox is checked
	const firmwareOptions = $derived.by(() => {
		const all = facets?.ver_sw_release_str ?? [];
		// Normalize: ensure v prefix, strip -release suffix (legacy format)
		const normalized = [...new Set(all.map(v => {
			let s = v.startsWith('v') ? v : `v${v}`;
			return s.replace(/-release$/, '');
		}))].sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
		if (includePreReleases) return normalized;
		// Stable = just vX.Y.Z with no suffix
		return normalized.filter(v => /^v\d+\.\d+\.\d+$/.test(v));
	});
	let durationMin = $state('');
	let durationMax = $state('');

	let debounceTimers: Record<string, ReturnType<typeof setTimeout>> = {};

	// Sync local state when filters change externally
	$effect(() => {
		hwValue = filters.ver_hw ?? '';
		fwValue = filters.ver_sw_release_str ?? '';
		locationValue = filters.location_name ?? '';
		durationMin = filters.flight_duration_min != null ? String(Math.round(filters.flight_duration_min / 60)) : '';
		durationMax = filters.flight_duration_max != null ? String(Math.round(filters.flight_duration_max / 60)) : '';
	});

	// Fetch facets on mount
	$effect(() => {
		if (!facetsLoaded) {
			facetsLoaded = true;
			getFilterFacets()
				.then((f) => { facets = f; })
				.catch(() => { /* facets are optional, degrade gracefully */ });
		}
	});

	function debouncedChange(field: keyof ListFilters, value: string, parseNum = false) {
		if (debounceTimers[field]) clearTimeout(debounceTimers[field]);
		debounceTimers[field] = setTimeout(() => {
			if (parseNum) {
				const num = value ? Number(value) : undefined;
				onChange({ [field]: (num != null && !isNaN(num)) ? num : undefined });
			} else {
				onChange({ [field]: value || undefined });
			}
		}, 300);
	}

	function handleSelectChange(field: keyof ListFilters, e: Event) {
		const val = (e.target as HTMLSelectElement).value;
		onChange({ [field]: val || undefined });
	}

	function handleGpsChange(e: Event) {
		const val = (e.target as HTMLSelectElement).value;
		if (val === 'true') onChange({ has_gps: true });
		else if (val === 'false') onChange({ has_gps: false });
		else onChange({ has_gps: undefined });
	}

	function handleDateChange(field: 'date_from' | 'date_to', e: Event) {
		const val = (e.target as HTMLInputElement).value;
		onChange({ [field]: val || undefined });
	}

	function clearAll() {
		hwValue = '';
		fwValue = '';
		locationValue = '';
		durationMin = '';
		durationMax = '';
		for (const key of Object.keys(debounceTimers)) {
			clearTimeout(debounceTimers[key]);
		}
		onChange({
			vehicle_type: undefined,
			ver_hw: undefined,
			ver_sw_release_str: undefined,
			location_name: undefined,
			flight_duration_min: undefined,
			flight_duration_max: undefined,
			date_from: undefined,
			date_to: undefined,
			vibration_status: undefined,
			has_gps: undefined,
			tag: undefined,
		});
	}

	const hasAdvancedFilters = $derived(
		!!filters.vehicle_type ||
		!!filters.ver_hw ||
		!!filters.ver_sw_release_str ||
		!!filters.location_name ||
		filters.flight_duration_min != null ||
		filters.flight_duration_max != null ||
		!!filters.date_from ||
		!!filters.date_to ||
		!!filters.vibration_status ||
		filters.has_gps != null ||
		!!filters.tag
	);

	const gpsSelectValue = $derived(
		filters.has_gps === true ? 'true' : filters.has_gps === false ? 'false' : ''
	);

	const inputClass = 'block w-full rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none';
	const selectClass = 'block w-full rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none';
	const labelClass = 'block text-xs font-medium text-gray-500 mb-1';
</script>

<div>
	<div class="flex items-center gap-3">
		<button
			type="button"
			onclick={() => { open = !open; }}
			class="inline-flex items-center gap-1.5 text-sm font-medium text-gray-600 hover:text-gray-900"
		>
			<svg
				class="size-4 transition-transform duration-200 {open ? 'rotate-180' : ''}"
				fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor"
			>
				<path stroke-linecap="round" stroke-linejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
			</svg>
			Advanced Filters
			{#if hasAdvancedFilters}
				<span class="inline-flex items-center justify-center size-5 rounded-full bg-indigo-100 text-xs font-medium text-indigo-700">!</span>
			{/if}
		</button>

		{#if hasAdvancedFilters}
			<button
				type="button"
				onclick={clearAll}
				class="text-xs text-indigo-600 hover:text-indigo-500"
			>
				Clear all
			</button>
		{/if}
	</div>

	{#if open}
		<div
			class="mt-3 overflow-visible transition-all duration-200"
		>
			<div class="grid grid-cols-1 lg:grid-cols-2 gap-4 rounded-lg border border-gray-200 bg-gray-50 p-4">
				<!-- Vehicle Type -->
				<div>
					<label for="filter-vehicle-type" class={labelClass}>Vehicle Type</label>
					<select
						id="filter-vehicle-type"
						value={filters.vehicle_type ?? ''}
						onchange={(e) => handleSelectChange('vehicle_type', e)}
						class={selectClass}
					>
						<option value="">All</option>
						{#if facets?.vehicle_type}
							{#each facets.vehicle_type as vt}
								<option value={vt}>{vt}</option>
							{/each}
						{/if}
					</select>
				</div>

				<!-- Hardware -->
				<Combobox
					label="Hardware"
					options={facets?.ver_hw ?? []}
					value={hwValue}
					placeholder="e.g. Pixhawk 6C"
					onChange={(v) => { hwValue = v; debouncedChange('ver_hw', v); }}
				/>

				<!-- Firmware -->
				<div>
					<Combobox
						label="Firmware"
						options={firmwareOptions}
						value={fwValue}
						placeholder="e.g. v1.14.0"
						onChange={(v) => { fwValue = v; debouncedChange('ver_sw_release_str', v); }}
					/>
					<label class="flex items-center gap-1.5 mt-1.5 text-xs text-gray-500 cursor-pointer select-none">
						<input
							type="checkbox"
							bind:checked={includePreReleases}
							class="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500 size-3.5"
						/>
						Include pre-releases
					</label>
				</div>

				<!-- Location -->
				<div>
					<label for="filter-location" class={labelClass}>Location</label>
					<input
						id="filter-location"
						type="text"
						placeholder="e.g. Zurich"
						value={locationValue}
						oninput={(e) => { locationValue = (e.target as HTMLInputElement).value; debouncedChange('location_name', locationValue); }}
						class={inputClass}
					/>
				</div>

				<!-- Flight Duration -->
				<div>
					<label for="filter-duration-min" class={labelClass}>Flight Duration (minutes)</label>
					<div class="flex items-center gap-2">
						<input
							id="filter-duration-min"
							type="number"
							placeholder="Min"
							min="0"
							value={durationMin}
							oninput={(e) => { durationMin = (e.target as HTMLInputElement).value; const mins = Number(durationMin); onChange({ flight_duration_min: durationMin && !isNaN(mins) ? mins * 60 : undefined }); }}
							class={inputClass}
						/>
						<span class="text-gray-400 text-sm">to</span>
						<input
							type="number"
							placeholder="Max"
							min="0"
							value={durationMax}
							oninput={(e) => { durationMax = (e.target as HTMLInputElement).value; const mins = Number(durationMax); onChange({ flight_duration_max: durationMax && !isNaN(mins) ? mins * 60 : undefined }); }}
							class={inputClass}
						/>
					</div>
				</div>

				<!-- Date Range -->
				<div>
					<label for="filter-date-from" class={labelClass}>Date Range</label>
					<div class="flex items-center gap-2">
						<input
							id="filter-date-from"
							type="date"
							value={filters.date_from ?? ''}
							onchange={(e) => handleDateChange('date_from', e)}
							class={inputClass}
						/>
						<span class="text-gray-400 text-sm">to</span>
						<input
							type="date"
							value={filters.date_to ?? ''}
							onchange={(e) => handleDateChange('date_to', e)}
							class={inputClass}
						/>
					</div>
				</div>

				<!-- Vibration Status -->
				<div>
					<label for="filter-vibration" class={labelClass}>Vibration Status</label>
					<select
						id="filter-vibration"
						value={filters.vibration_status ?? ''}
						onchange={(e) => handleSelectChange('vibration_status', e)}
						class={selectClass}
					>
						<option value="">All</option>
						{#if facets?.vibration_status}
							{#each facets.vibration_status as vs}
								<option value={vs}>{vs}</option>
							{/each}
						{/if}
					</select>
				</div>

				<!-- Has GPS -->
				<div>
					<label for="filter-gps" class={labelClass}>Has GPS</label>
					<select
						id="filter-gps"
						value={gpsSelectValue}
						onchange={handleGpsChange}
						class={selectClass}
					>
						<option value="">All</option>
						<option value="true">Yes</option>
						<option value="false">No</option>
					</select>
				</div>
			</div>
		</div>
	{/if}
</div>
