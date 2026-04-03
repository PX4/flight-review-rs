<script lang="ts">
	import type { ListFilters } from '$lib/types';

	let { filters, onChange }: {
		filters: ListFilters;
		onChange: (filters: Partial<ListFilters>) => void;
	} = $props();

	interface Chip {
		key: keyof ListFilters;
		label: string;
		clearValue: undefined;
	}

	let chips = $derived.by((): Chip[] => {
		const result: Chip[] = [];

		if (filters.sys_name) {
			result.push({ key: 'sys_name', label: `Vehicle: ${filters.sys_name}`, clearValue: undefined });
		}
		if (filters.ver_hw) {
			result.push({ key: 'ver_hw', label: `Hardware: ${filters.ver_hw}`, clearValue: undefined });
		}
		if (filters.vehicle_type) {
			result.push({ key: 'vehicle_type', label: `Type: ${filters.vehicle_type}`, clearValue: undefined });
		}
		if (filters.ver_sw_release_str) {
			result.push({ key: 'ver_sw_release_str', label: `Firmware: ${filters.ver_sw_release_str}`, clearValue: undefined });
		}
		if (filters.location_name) {
			result.push({ key: 'location_name', label: `Location: ${filters.location_name}`, clearValue: undefined });
		}
		if (filters.flight_duration_min != null) {
			result.push({ key: 'flight_duration_min', label: `Duration: >${Math.round(filters.flight_duration_min / 60)}m`, clearValue: undefined });
		}
		if (filters.flight_duration_max != null) {
			result.push({ key: 'flight_duration_max', label: `Duration: <${Math.round(filters.flight_duration_max / 60)}m`, clearValue: undefined });
		}
		if (filters.date_from) {
			result.push({ key: 'date_from', label: `From: ${filters.date_from}`, clearValue: undefined });
		}
		if (filters.date_to) {
			result.push({ key: 'date_to', label: `To: ${filters.date_to}`, clearValue: undefined });
		}
		if (filters.vibration_status) {
			result.push({ key: 'vibration_status', label: `Vibration: ${filters.vibration_status}`, clearValue: undefined });
		}
		if (filters.has_gps != null) {
			result.push({ key: 'has_gps', label: `GPS: ${filters.has_gps ? 'Yes' : 'No'}`, clearValue: undefined });
		}
		if (filters.tag) {
			result.push({ key: 'tag', label: `Tag: ${filters.tag}`, clearValue: undefined });
		}

		return result;
	});

	function dismiss(chip: Chip) {
		onChange({ [chip.key]: chip.clearValue });
	}
</script>

{#if chips.length > 0}
	<div class="flex flex-wrap gap-2">
		{#each chips as chip (chip.key)}
			<span class="inline-flex items-center gap-1 rounded-full bg-indigo-50 px-3 py-1 text-xs font-medium text-indigo-700 ring-1 ring-indigo-200">
				{chip.label}
				<button
					type="button"
					aria-label="Remove {chip.label} filter"
					onclick={() => dismiss(chip)}
					class="ml-0.5 inline-flex items-center rounded-full p-0.5 text-indigo-400 hover:bg-indigo-100 hover:text-indigo-600"
				>
					<svg class="size-3" fill="none" viewBox="0 0 24 24" stroke-width="2.5" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
					</svg>
				</button>
			</span>
		{/each}
	</div>
{/if}
