<script lang="ts">
	import { getContext } from 'svelte';
	import type { FlightMetadata } from '$lib/types';
	import SystemDiagnosticsPanel from '$lib/components/viewer/SystemDiagnosticsPanel.svelte';

	const ctx = getContext<{ metadata: FlightMetadata }>('log-viewer');
</script>

<SystemDiagnosticsPanel multiInfo={ctx.metadata?.multi_info ?? {}} />
