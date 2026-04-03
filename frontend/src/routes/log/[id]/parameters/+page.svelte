<script lang="ts">
	import { getContext } from 'svelte';
	import type { FlightMetadata } from '$lib/types';
	import ParamDiffPanel from '$lib/components/viewer/ParamDiffPanel.svelte';

	const ctx = getContext<{ metadata: FlightMetadata }>('log-viewer');
</script>

<ParamDiffPanel
	diffs={ctx.metadata?.analysis?.non_default_params ?? []}
	changedParams={ctx.metadata?.changed_parameters ?? []}
	allParameters={ctx.metadata?.parameters ?? {}}
	defaultParameters={ctx.metadata?.default_parameters ?? {}}
/>
