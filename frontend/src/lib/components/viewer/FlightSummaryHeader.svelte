<script lang="ts">
	import type { FlightMetadata } from '$lib/types';
	import { formatDuration } from '$lib/utils/formatters';
	import { getHardwareName } from '$lib/utils/hardwareNames';

	let { metadata, logId, vehicleType, locationName } = $props<{ metadata: FlightMetadata; logId: string; vehicleType?: string | null; locationName?: string | null }>();

	// Parse location_name: "City, Country [CC]" → { city, country, countryCode, flag }
	const location = $derived.by(() => {
		if (!locationName) return null;
		const ccMatch = locationName.match(/\[([A-Z]{2})\]$/);
		const cc = ccMatch ? ccMatch[1] : null;
		const name = ccMatch ? locationName.replace(/\s*\[[A-Z]{2}\]$/, '') : locationName;
		const parts = name.split(', ');
		const city = parts.length > 1 ? parts[0] : null;
		const country = parts.length > 1 ? parts.slice(1).join(', ') : parts[0];
		// Convert country code to flag emoji (regional indicator symbols)
		const flag = cc ? String.fromCodePoint(...[...cc].map(c => 0x1F1E6 + c.charCodeAt(0) - 65)) : null;
		return { city, country, flag };
	});

	const stats = $derived(metadata.analysis?.stats);
	const battery = $derived(metadata.analysis?.battery);
	const vibration = $derived(metadata.analysis?.vibration);
	const gps = $derived(metadata.analysis?.gps_quality);

	function formatDistance(meters: number | undefined): string {
		if (meters == null) return '\u2014';
		if (meters < 1000) return `${Math.round(meters)} m`;
		return `${(meters / 1000).toFixed(1)} km`;
	}

	function formatAltitude(meters: number | undefined): string {
		if (meters == null) return '\u2014';
		return `${Math.round(meters)} m`;
	}

	function formatSpeed(mps: number | undefined): string {
		if (mps == null) return '\u2014';
		return `${mps.toFixed(1)} m/s`;
	}

	function formatBattery(mah: number | null | undefined): string {
		if (mah == null) return '\u2014';
		return `${Math.round(mah).toLocaleString()} mAh`;
	}

	function vibrationBadge(status: string | undefined): { text: string; bg: string; fg: string } {
		if (!status) return { text: '\u2014', bg: '', fg: 'text-gray-500' };
		const lower = status.toLowerCase();
		if (lower === 'good') return { text: 'Good', bg: 'bg-emerald-50 ring-1 ring-emerald-600/20', fg: 'text-emerald-700' };
		if (lower === 'warning') return { text: 'Warning', bg: 'bg-amber-50 ring-1 ring-amber-600/20', fg: 'text-amber-700' };
		return { text: 'Critical', bg: 'bg-red-50 ring-1 ring-red-600/20', fg: 'text-red-700' };
	}

	function vehicleIconPath(type: string | null | undefined): string {
		switch (type?.toLowerCase()) {
			case 'multirotor': return '/icons/quadrotor.svg';
			case 'fixed wing': return '/icons/fixedwing.svg';
			case 'vtol': return '/icons/vtol.svg';
			case 'rover': return '/icons/rover.svg';
			case 'boat': return '/icons/submarine.svg';
			case 'submarine': return '/icons/submarine.svg';
			default: return '/icons/unknown.svg';
		}
	}

	const vibe = $derived(vibrationBadge(vibration?.status));
</script>

<div class="border-b border-gray-200 bg-gray-50">
	<!-- Mobile/Tablet: horizontal scroll, single row -->
	<dl class="flex overflow-x-auto lg:hidden divide-x divide-gray-200 scrollbar-none">
		<div class="shrink-0 px-3 py-1.5 flex items-center gap-2">
			<img src={vehicleIconPath(vehicleType)} alt={vehicleType ?? ''} class="size-8 opacity-70" />
			<dd class="text-xs font-semibold text-gray-900 whitespace-nowrap">{vehicleType ?? metadata.sys_name ?? '\u2014'}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5 flex items-center gap-1.5">
			{#if location}
				{#if location.flag}<span class="text-base">{location.flag}</span>{/if}
				<div class="flex flex-col">
					{#if location.country}<dd class="text-xs font-semibold text-gray-900 whitespace-nowrap">{location.country}</dd>{/if}
					{#if location.city}<dd class="text-[10px] text-gray-500 whitespace-nowrap">{location.city}</dd>{/if}
				</div>
			{:else}
				<svg class="size-4 text-gray-300" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M15 10.5a3 3 0 11-6 0 3 3 0 016 0z" />
					<path stroke-linecap="round" stroke-linejoin="round" d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z" />
					<path stroke-linecap="round" stroke-linejoin="round" d="M3 3l18 18" />
				</svg>
				<dd class="text-xs text-gray-400 whitespace-nowrap">No Location</dd>
			{/if}
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Hardware</dt>
			<dd class="text-xs font-medium text-gray-700 whitespace-nowrap">{getHardwareName(metadata.ver_hw)}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Duration</dt>
			<dd class="text-xs font-semibold text-gray-900 whitespace-nowrap">{formatDuration(metadata.flight_duration_s)}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Distance</dt>
			<dd class="text-xs font-medium text-gray-700 whitespace-nowrap">{formatDistance(stats?.total_distance_m)}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Max Alt</dt>
			<dd class="text-xs font-medium text-gray-700 whitespace-nowrap">{formatAltitude(stats?.max_altitude_diff_m)}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Speed</dt>
			<dd class="text-xs font-medium text-gray-700 whitespace-nowrap">{formatSpeed(stats?.max_speed_m_s)}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Battery</dt>
			<dd class="text-xs font-medium text-gray-700 whitespace-nowrap">{formatBattery(battery?.discharged_mah)}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">Vibration</dt>
			<dd class="mt-0.5">
				{#if vibe.bg}
					<span class="inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium {vibe.bg} {vibe.fg}">{vibe.text}</span>
				{:else}
					<span class="text-xs text-gray-500">{vibe.text}</span>
				{/if}
			</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5">
			<dt class="text-[10px] text-gray-500">GPS</dt>
			<dd class="text-xs font-medium text-gray-700 whitespace-nowrap">{gps?.max_satellites != null ? `${gps.max_satellites} sats` : '\u2014'}</dd>
		</div>
		<div class="shrink-0 px-3 py-1.5 flex items-center">
			<a
				href="/api/logs/{logId}/download"
				class="rounded-md bg-white px-2 py-1 text-xs font-medium text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50 flex items-center gap-1"
			>
				<svg class="size-3" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12M12 16.5V3" />
				</svg>
				.ulg
			</a>
		</div>
	</dl>
	<!-- Desktop: flex layout -->
	<dl class="hidden lg:flex lg:divide-x divide-gray-200 lg:px-0">
		<div class="px-2 lg:px-4 py-2 lg:py-3 flex items-center gap-3">
			<img src={vehicleIconPath(vehicleType)} alt={vehicleType ?? ''} class="size-10 opacity-70" />
			<dd class="text-sm font-semibold text-gray-900 whitespace-nowrap">{vehicleType ?? metadata.sys_name ?? '\u2014'}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3 flex items-center gap-2">
			{#if location}
				{#if location.flag}<span class="text-2xl">{location.flag}</span>{/if}
				<div class="flex flex-col">
					{#if location.country}<dd class="text-sm font-semibold text-gray-900 whitespace-nowrap">{location.country}</dd>{/if}
					{#if location.city}<dd class="text-xs text-gray-500 whitespace-nowrap">{location.city}</dd>{/if}
				</div>
			{:else}
				<svg class="size-5 text-gray-300" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M15 10.5a3 3 0 11-6 0 3 3 0 016 0z" />
					<path stroke-linecap="round" stroke-linejoin="round" d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z" />
					<path stroke-linecap="round" stroke-linejoin="round" d="M3 3l18 18" />
				</svg>
				<dd class="text-sm text-gray-400 whitespace-nowrap">No Location</dd>
			{/if}
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Hardware</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5 truncate" title={metadata.ver_hw ?? ''}>{getHardwareName(metadata.ver_hw)}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Firmware</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5 truncate">{metadata.ver_sw_release_str ?? '\u2014'}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Duration</dt>
			<dd class="text-sm font-semibold text-gray-900 mt-0.5">{formatDuration(metadata.flight_duration_s)}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Distance</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatDistance(stats?.total_distance_m)}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Max Alt</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatAltitude(stats?.max_altitude_diff_m)}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Max Speed</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatSpeed(stats?.max_speed_m_s)}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Battery</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatBattery(battery?.discharged_mah)}</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">Vibration</dt>
			<dd class="mt-0.5">
				{#if vibe.bg}
					<span class="inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium {vibe.bg} {vibe.fg}">{vibe.text}</span>
				{:else}
					<span class="text-sm text-gray-500">{vibe.text}</span>
				{/if}
			</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3">
			<dt class="text-xs text-gray-500">GPS</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">
				{gps?.max_satellites != null ? `${gps.max_satellites} sats` : '\u2014'}
			</dd>
		</div>
		<div class="px-2 lg:px-4 py-2 lg:py-3 flex items-end">
			<a
				href="/api/logs/{logId}/download"
				class="rounded-md bg-white px-3 py-1.5 text-xs font-medium text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50 flex items-center gap-1.5"
			>
				<svg class="size-3.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
					<path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12M12 16.5V3" />
				</svg>
				.ulg
			</a>
		</div>
	</dl>
</div>
