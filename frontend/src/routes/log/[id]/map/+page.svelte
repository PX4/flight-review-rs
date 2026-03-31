<script lang="ts">
	import { getContext } from 'svelte';
	import type { FlightMetadata } from '$lib/types';
	import MapPanel from '$lib/components/viewer/MapPanel.svelte';

	const ctx = getContext<{ metadata: FlightMetadata }>('log-viewer');
</script>

<MapPanel
	track={ctx.metadata?.analysis?.gps_track ?? []}
	modes={ctx.metadata?.analysis?.flight_modes ?? []}
/>
