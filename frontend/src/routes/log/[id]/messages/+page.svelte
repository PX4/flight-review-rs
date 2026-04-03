<script lang="ts">
	import { getContext } from 'svelte';
	import type { FlightMetadata } from '$lib/types';
	import LogMessagesPanel from '$lib/components/viewer/LogMessagesPanel.svelte';

	const ctx = getContext<{ metadata: FlightMetadata }>('log-viewer');

	const flightStartUs = $derived.by(() => {
		const modes = ctx.metadata?.analysis?.flight_modes;
		if (modes && modes.length > 0) return modes[0].start_us;
		return 0;
	});
</script>

<LogMessagesPanel
	messages={ctx.metadata?.logged_messages ?? []}
	{flightStartUs}
/>
