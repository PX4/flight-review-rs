<script lang="ts">
	import type { TrackPoint, FlightModeSegment } from '$lib/types';
	import { cursorTimestamp } from '$lib/stores/plotSync';
	import { getModeColor } from '$lib/utils/modeColors';

	let { track, modes }: { track: TrackPoint[]; modes: FlightModeSegment[] } = $props();

	const PADDING = 20;
	const SVG_SIZE = 600;

	let bounds = $derived.by(() => {
		if (track.length === 0) return null;
		let minLat = Infinity, maxLat = -Infinity;
		let minLon = Infinity, maxLon = -Infinity;
		for (const pt of track) {
			if (pt.lat_deg < minLat) minLat = pt.lat_deg;
			if (pt.lat_deg > maxLat) maxLat = pt.lat_deg;
			if (pt.lon_deg < minLon) minLon = pt.lon_deg;
			if (pt.lon_deg > maxLon) maxLon = pt.lon_deg;
		}
		const latRange = maxLat - minLat || 0.0001;
		const lonRange = maxLon - minLon || 0.0001;
		return { minLat, maxLat, minLon, maxLon, latRange, lonRange };
	});

	function toSvg(lat: number, lon: number): { x: number; y: number } {
		if (!bounds) return { x: 0, y: 0 };
		const usable = SVG_SIZE - 2 * PADDING;
		const scale = Math.min(usable / bounds.lonRange, usable / bounds.latRange);
		const cx = (bounds.minLon + bounds.maxLon) / 2;
		const cy = (bounds.minLat + bounds.maxLat) / 2;
		const x = PADDING + usable / 2 + (lon - cx) * scale;
		const y = PADDING + usable / 2 - (lat - cy) * scale;
		return { x, y };
	}

	function getModeForTimestamp(ts: number): number {
		for (const seg of modes) {
			if (ts >= seg.start_us && ts <= seg.end_us) return seg.mode_id;
		}
		return -1;
	}

	let segments = $derived.by(() => {
		if (track.length < 2 || !bounds) return [];
		const result: { points: string; color: string }[] = [];
		let currentModeId = track[0].mode_id;
		let currentColor = getModeColor(currentModeId);
		let pts: string[] = [];

		for (const pt of track) {
			const { x, y } = toSvg(pt.lat_deg, pt.lon_deg);
			if (pt.mode_id !== currentModeId) {
				// close previous segment with this point for continuity
				pts.push(`${x},${y}`);
				result.push({ points: pts.join(' '), color: currentColor });
				currentModeId = pt.mode_id;
				currentColor = getModeColor(currentModeId);
				pts = [`${x},${y}`];
			} else {
				pts.push(`${x},${y}`);
			}
		}
		if (pts.length > 1) {
			result.push({ points: pts.join(' '), color: currentColor });
		}
		return result;
	});

	let startPos = $derived(track.length > 0 ? toSvg(track[0].lat_deg, track[0].lon_deg) : null);
	let endPos = $derived(track.length > 0 ? toSvg(track[track.length - 1].lat_deg, track[track.length - 1].lon_deg) : null);

	let cursorPos = $derived.by(() => {
		const ts = $cursorTimestamp;
		if (ts == null || track.length === 0) return null;
		// find the closest track point
		let closest = track[0];
		let bestDist = Math.abs(closest.timestamp_us - ts);
		for (let i = 1; i < track.length; i++) {
			const d = Math.abs(track[i].timestamp_us - ts);
			if (d < bestDist) {
				bestDist = d;
				closest = track[i];
			}
		}
		const { x, y } = toSvg(closest.lat_deg, closest.lon_deg);
		return { x, y, modeId: closest.mode_id };
	});
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 overflow-hidden">
	{#if track.length === 0}
		<div class="flex flex-col items-center justify-center py-24 text-center">
			<svg class="size-12 text-gray-300 mb-4" fill="none" viewBox="0 0 24 24" stroke-width="1" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M9 6.75V15m6-6v8.25m.503 3.498l4.875-2.437c.381-.19.622-.58.622-1.006V4.82c0-.836-.88-1.38-1.628-1.006l-3.869 1.934c-.317.159-.69.159-1.006 0L9.503 3.252a1.125 1.125 0 00-1.006 0L3.622 5.689C3.24 5.88 3 6.27 3 6.695V19.18c0 .836.88 1.38 1.628 1.006l3.869-1.934c.317-.159.69-.159 1.006 0l4.994 2.497c.317.158.69.158 1.006 0z" />
			</svg>
			<h3 class="text-sm font-medium text-gray-900">No GPS data</h3>
			<p class="mt-1 text-sm text-gray-500">This log does not contain GPS track data.</p>
		</div>
	{:else}
		<div class="bg-gray-50 p-4">
			<svg viewBox="0 0 {SVG_SIZE} {SVG_SIZE}" class="w-full h-auto">
				<!-- Track segments colored by flight mode -->
				{#each segments as seg}
					<polyline
						points={seg.points}
						fill="none"
						stroke={seg.color}
						stroke-width="2.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				{/each}

				<!-- Start marker (green) -->
				{#if startPos}
					<circle cx={startPos.x} cy={startPos.y} r="6" fill="#22c55e" stroke="white" stroke-width="2" />
				{/if}

				<!-- End marker (red) -->
				{#if endPos}
					<circle cx={endPos.x} cy={endPos.y} r="6" fill="#ef4444" stroke="white" stroke-width="2" />
				{/if}

				<!-- Animated cursor marker -->
				{#if cursorPos}
					<circle cx={cursorPos.x} cy={cursorPos.y} r="5" fill={getModeColor(cursorPos.modeId)} stroke="white" stroke-width="2" opacity="0.9">
						<animate attributeName="r" values="5;7;5" dur="1.5s" repeatCount="indefinite" />
					</circle>
				{/if}
			</svg>
		</div>

		<div class="px-4 py-2 text-xs text-gray-400 text-center border-t border-gray-100">
			Full interactive map coming soon
		</div>
	{/if}
</div>
