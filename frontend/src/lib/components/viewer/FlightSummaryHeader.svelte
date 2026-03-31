<script lang="ts">
	import type { FlightMetadata } from '$lib/types';
	import { formatDuration } from '$lib/utils/formatters';
	import { getHardwareName } from '$lib/utils/hardwareNames';

	let { metadata, logId } = $props<{ metadata: FlightMetadata; logId: string }>();

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

	const vibe = $derived(vibrationBadge(vibration?.status));
</script>

<div class="border-b border-gray-200 bg-gray-50 overflow-x-auto">
	<dl class="flex divide-x divide-gray-200 min-w-max lg:min-w-full">
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Vehicle</dt>
			<dd class="text-sm font-semibold text-gray-900 mt-0.5">{metadata.sys_name ?? '\u2014'}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Hardware</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5 truncate" title={metadata.ver_hw ?? ''}>{getHardwareName(metadata.ver_hw)}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Firmware</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{metadata.ver_sw_release_str ?? '\u2014'}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Duration</dt>
			<dd class="text-sm font-semibold text-gray-900 mt-0.5">{formatDuration(metadata.flight_duration_s)}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Distance</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatDistance(stats?.total_distance_m)}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Max Alt</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatAltitude(stats?.max_altitude_diff_m)}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Max Speed</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatSpeed(stats?.max_speed_m_s)}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Battery</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">{formatBattery(battery?.discharged_mah)}</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">Vibration</dt>
			<dd class="mt-0.5">
				{#if vibe.bg}
					<span class="inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium {vibe.bg} {vibe.fg}">{vibe.text}</span>
				{:else}
					<span class="text-sm text-gray-500">{vibe.text}</span>
				{/if}
			</dd>
		</div>
		<div class="px-4 py-3">
			<dt class="text-xs text-gray-500">GPS</dt>
			<dd class="text-sm font-medium text-gray-700 mt-0.5">
				{gps?.max_satellites != null ? `${gps.max_satellites} sats` : '\u2014'}
			</dd>
		</div>
		<div class="px-4 py-3 flex items-center">
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
