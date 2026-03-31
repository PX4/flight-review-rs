<script lang="ts">
	import type { FlightMetadata, TrackPoint } from '$lib/types';
	import { getModeColor } from '$lib/utils/modeColors';
	import { getMetadata } from '$lib/api';

	let { logId, width = 160, height = 100 }: { logId: string; width?: number; height?: number } = $props();

	let canvas: HTMLCanvasElement | undefined = $state();
	let sentinel: HTMLDivElement | undefined = $state();
	let status = $state<'idle' | 'loading' | 'loaded' | 'no-gps' | 'error'>('idle');

	// Module-level cache shared across all instances
	const metadataCache = new Map<string, FlightMetadata>();

	function drawTrack(ctx: CanvasRenderingContext2D, track: TrackPoint[], w: number, h: number) {
		if (track.length < 2) return;

		const pad = 8;
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
		const usableW = w - 2 * pad;
		const usableH = h - 2 * pad;
		const scale = Math.min(usableW / lonRange, usableH / latRange);
		const cx = (minLon + maxLon) / 2;
		const cy = (minLat + maxLat) / 2;

		function toX(lon: number) { return pad + usableW / 2 + (lon - cx) * scale; }
		function toY(lat: number) { return pad + usableH / 2 - (lat - cy) * scale; }

		ctx.clearRect(0, 0, w, h);
		ctx.lineWidth = 1.5;
		ctx.lineCap = 'round';
		ctx.lineJoin = 'round';

		let prevModeId = track[0].mode_id;
		ctx.beginPath();
		ctx.strokeStyle = getModeColor(prevModeId);
		ctx.moveTo(toX(track[0].lon_deg), toY(track[0].lat_deg));

		for (let i = 1; i < track.length; i++) {
			const pt = track[i];
			const x = toX(pt.lon_deg);
			const y = toY(pt.lat_deg);
			if (pt.mode_id !== prevModeId) {
				ctx.lineTo(x, y);
				ctx.stroke();
				ctx.beginPath();
				ctx.strokeStyle = getModeColor(pt.mode_id);
				ctx.moveTo(x, y);
				prevModeId = pt.mode_id;
			} else {
				ctx.lineTo(x, y);
			}
		}
		ctx.stroke();

		// Start marker
		const sx = toX(track[0].lon_deg);
		const sy = toY(track[0].lat_deg);
		ctx.beginPath();
		ctx.arc(sx, sy, 3, 0, Math.PI * 2);
		ctx.fillStyle = '#22c55e';
		ctx.fill();

		// End marker
		const ex = toX(track[track.length - 1].lon_deg);
		const ey = toY(track[track.length - 1].lat_deg);
		ctx.beginPath();
		ctx.arc(ex, ey, 3, 0, Math.PI * 2);
		ctx.fillStyle = '#ef4444';
		ctx.fill();
	}

	async function loadAndDraw() {
		if (status !== 'idle') return;
		status = 'loading';
		try {
			let meta = metadataCache.get(logId);
			if (!meta) {
				meta = await getMetadata(logId);
				metadataCache.set(logId, meta);
			}
			const track = meta.analysis?.gps_track;
			if (!track || track.length < 2) {
				status = 'no-gps';
				return;
			}
			if (canvas) {
				const ctx = canvas.getContext('2d');
				if (ctx) {
					const dpr = window.devicePixelRatio || 1;
					canvas.width = width * dpr;
					canvas.height = height * dpr;
					ctx.scale(dpr, dpr);
					drawTrack(ctx, track, width, height);
				}
			}
			status = 'loaded';
		} catch {
			status = 'error';
		}
	}

	$effect(() => {
		if (!sentinel) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0]?.isIntersecting) {
					loadAndDraw();
					observer.disconnect();
				}
			},
			{ rootMargin: '200px' }
		);
		observer.observe(sentinel);
		return () => observer.disconnect();
	});
</script>

<div
	bind:this={sentinel}
	class="rounded bg-gray-50 ring-1 ring-gray-200 overflow-hidden flex items-center justify-center"
	style="width: {width}px; height: {height}px;"
>
	{#if status === 'idle' || status === 'loading'}
		<div class="text-xs text-gray-300">
			{#if status === 'loading'}
				<svg class="animate-spin size-4" fill="none" viewBox="0 0 24 24">
					<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
					<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z"></path>
				</svg>
			{/if}
		</div>
	{:else if status === 'no-gps'}
		<span class="text-xs text-gray-300">No GPS</span>
	{:else if status === 'error'}
		<span class="text-xs text-red-300">Error</span>
	{/if}
	<canvas
		bind:this={canvas}
		style="width: {width}px; height: {height}px; {status === 'loaded' ? '' : 'display: none;'}"
	></canvas>
</div>
