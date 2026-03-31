<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { PUBLIC_MAPBOX_TOKEN } from '$env/static/public';
	import type { TrackPoint, FlightModeSegment } from '$lib/types';
	import { cursorTimestamp, timeRange } from '$lib/stores/plotSync';
	import { getModeColor } from '$lib/utils/modeColors';

	let { track, modes }: { track: TrackPoint[]; modes: FlightModeSegment[] } = $props();

	let mapContainer = $state<HTMLDivElement>(undefined!);
	let map: any = null;
	let cursorMarker: any = null;
	let loaded = $state(false);
	let error = $state('');

	function buildTrackGeoJSON(): GeoJSON.FeatureCollection {
		if (track.length < 2) {
			return { type: 'FeatureCollection', features: [] };
		}
		const features: GeoJSON.Feature[] = [];
		let currentModeId = track[0].mode_id;
		let coords: [number, number][] = [[track[0].lon_deg, track[0].lat_deg]];

		for (let i = 1; i < track.length; i++) {
			const pt = track[i];
			if (pt.mode_id !== currentModeId) {
				coords.push([pt.lon_deg, pt.lat_deg]);
				features.push({
					type: 'Feature',
					properties: { color: getModeColor(currentModeId) },
					geometry: { type: 'LineString', coordinates: coords },
				});
				currentModeId = pt.mode_id;
				coords = [[pt.lon_deg, pt.lat_deg]];
			} else {
				coords.push([pt.lon_deg, pt.lat_deg]);
			}
		}
		if (coords.length > 1) {
			features.push({
				type: 'Feature',
				properties: { color: getModeColor(currentModeId) },
				geometry: { type: 'LineString', coordinates: coords },
			});
		}
		return { type: 'FeatureCollection', features };
	}

	function getBounds(): [[number, number], [number, number]] | null {
		if (track.length === 0) return null;
		let minLon = Infinity, maxLon = -Infinity;
		let minLat = Infinity, maxLat = -Infinity;
		for (const pt of track) {
			if (pt.lon_deg < minLon) minLon = pt.lon_deg;
			if (pt.lon_deg > maxLon) maxLon = pt.lon_deg;
			if (pt.lat_deg < minLat) minLat = pt.lat_deg;
			if (pt.lat_deg > maxLat) maxLat = pt.lat_deg;
		}
		return [[minLon, minLat], [maxLon, maxLat]];
	}

	function findClosestPoint(timestampUs: number): TrackPoint | null {
		if (track.length === 0) return null;
		let closest = track[0];
		let bestDist = Math.abs(closest.timestamp_us - timestampUs);
		for (let i = 1; i < track.length; i++) {
			const d = Math.abs(track[i].timestamp_us - timestampUs);
			if (d < bestDist) {
				bestDist = d;
				closest = track[i];
			}
		}
		return closest;
	}

	function findClosestByLngLat(lng: number, lat: number): TrackPoint | null {
		if (track.length === 0) return null;
		let closest = track[0];
		let bestDist = (closest.lon_deg - lng) ** 2 + (closest.lat_deg - lat) ** 2;
		for (let i = 1; i < track.length; i++) {
			const d = (track[i].lon_deg - lng) ** 2 + (track[i].lat_deg - lat) ** 2;
			if (d < bestDist) {
				bestDist = d;
				closest = track[i];
			}
		}
		return closest;
	}

	onMount(async () => {
		if (track.length === 0) return;
		if (!PUBLIC_MAPBOX_TOKEN) {
			error = 'Mapbox token not configured';
			return;
		}

		try {
			const mapboxgl = await import('mapbox-gl');
			await import('mapbox-gl/dist/mapbox-gl.css');

			const mb = mapboxgl.default || mapboxgl;
			mb.accessToken = PUBLIC_MAPBOX_TOKEN;

			map = new mb.Map({
				container: mapContainer,
				style: 'mapbox://styles/mapbox/outdoors-v12',
				attributionControl: true,
			});

			map.on('load', () => {
				loaded = true;

				const geojson = buildTrackGeoJSON();
				map.addSource('track', { type: 'geojson', data: geojson });

				map.addLayer({
					id: 'track-line',
					type: 'line',
					source: 'track',
					paint: {
						'line-color': ['get', 'color'],
						'line-width': 3,
						'line-opacity': 0.9,
					},
					layout: {
						'line-cap': 'round',
						'line-join': 'round',
					},
				});

				// Start marker
				const startEl = document.createElement('div');
				startEl.className = 'w-4 h-4 rounded-full bg-emerald-500 border-2 border-white shadow-md';
				new mb.Marker({ element: startEl })
					.setLngLat([track[0].lon_deg, track[0].lat_deg])
					.setPopup(new mb.Popup({ offset: 10 }).setText('Start'))
					.addTo(map);

				// End marker
				const endEl = document.createElement('div');
				endEl.className = 'w-4 h-4 rounded-full bg-red-500 border-2 border-white shadow-md';
				new mb.Marker({ element: endEl })
					.setLngLat([track[track.length - 1].lon_deg, track[track.length - 1].lat_deg])
					.setPopup(new mb.Popup({ offset: 10 }).setText('End'))
					.addTo(map);

				// Cursor marker
				const cursorEl = document.createElement('div');
				cursorEl.className = 'w-3 h-3 rounded-full bg-indigo-500 border-2 border-white shadow-lg';
				cursorMarker = new mb.Marker({ element: cursorEl })
					.setLngLat([track[0].lon_deg, track[0].lat_deg])
					.addTo(map);

				// Fit bounds
				const bounds = getBounds();
				if (bounds) {
					map.fitBounds(bounds, { padding: 60, maxZoom: 17 });
				}

				// Click on track → update cursor timestamp
				map.on('click', 'track-line', (e: any) => {
					if (e.lngLat) {
						const closest = findClosestByLngLat(e.lngLat.lng, e.lngLat.lat);
						if (closest) {
							cursorTimestamp.set(closest.timestamp_us / 1e6);
						}
					}
				});

				map.on('mouseenter', 'track-line', () => {
					map.getCanvas().style.cursor = 'crosshair';
				});
				map.on('mouseleave', 'track-line', () => {
					map.getCanvas().style.cursor = '';
				});
			});
		} catch (e: any) {
			error = e.message || 'Failed to load map';
			console.error('MapPanel error:', e);
		}
	});

	// Sync cursor marker position
	$effect(() => {
		const ts = $cursorTimestamp;
		if (!cursorMarker || ts == null || track.length === 0) return;
		const pt = findClosestPoint(ts * 1e6);
		if (pt) {
			cursorMarker.setLngLat([pt.lon_deg, pt.lat_deg]);
		}
	});

	// Dim out-of-range track when zoomed
	$effect(() => {
		const range = $timeRange;
		if (!map || !loaded) return;
		const layer = map.getLayer('track-line');
		if (!layer) return;
		map.setPaintProperty('track-line', 'line-opacity', range ? 0.5 : 0.9);
	});

	onDestroy(() => {
		if (map) {
			map.remove();
			map = null;
		}
	});
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 overflow-hidden flex flex-col flex-1">
	{#if track.length === 0}
		<div class="flex flex-col items-center justify-center py-24 text-center">
			<svg class="size-12 text-gray-300 mb-4" fill="none" viewBox="0 0 24 24" stroke-width="1" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M9 6.75V15m6-6v8.25m.503 3.498l4.875-2.437c.381-.19.622-1.006V4.82c0-.836-.88-1.38-1.628-1.006l-3.869 1.934c-.317.159-.69.159-1.006 0L9.503 3.252a1.125 1.125 0 00-1.006 0L3.622 5.689C3.24 5.88 3 6.27 3 6.695V19.18c0 .836.88 1.38 1.628 1.006l3.869-1.934c.317-.159.69-.159 1.006 0l4.994 2.497c.317.158.69.158 1.006 0z" />
			</svg>
			<h3 class="text-sm font-medium text-gray-900">No GPS data</h3>
			<p class="mt-1 text-sm text-gray-500">This log does not contain GPS track data.</p>
		</div>
	{:else}
		{#if error}
			<div class="p-4 text-sm text-red-600">{error}</div>
		{/if}
		<div class="relative flex-1 min-h-[50vh] lg:min-h-0">
			<div bind:this={mapContainer} class="absolute inset-0 w-full h-full"></div>
			{#if !loaded && !error}
				<div class="absolute inset-0 flex items-center justify-center bg-gray-50">
					<div class="flex items-center gap-2 text-sm text-gray-500">
						<svg class="size-5 animate-spin text-indigo-500" fill="none" viewBox="0 0 24 24">
							<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
							<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
						</svg>
						Loading map...
					</div>
				</div>
			{/if}
		</div>
		<div class="flex items-center gap-4 px-4 py-2 border-t border-gray-100 text-xs text-gray-500">
			<span class="flex items-center gap-1">
				<span class="w-2.5 h-2.5 rounded-full bg-emerald-500 border border-white shadow-sm"></span>
				Start
			</span>
			<span class="flex items-center gap-1">
				<span class="w-2.5 h-2.5 rounded-full bg-red-500 border border-white shadow-sm"></span>
				End
			</span>
			<span class="flex items-center gap-1">
				<span class="w-2.5 h-2.5 rounded-full bg-indigo-500 border border-white shadow-sm"></span>
				Cursor
			</span>
		</div>
	{/if}
</div>
