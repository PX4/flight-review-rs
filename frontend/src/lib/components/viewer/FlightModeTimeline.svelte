<script lang="ts">
	import type { FlightModeSegment } from '$lib/types';
	import { getModeColor, getModeName } from '$lib/utils/modeColors';

	let { segments } = $props<{ segments: FlightModeSegment[] }>();

	const totalDuration = $derived(
		segments.reduce((sum: number, s: FlightModeSegment) => sum + s.duration_s, 0)
	);

	function flexValue(segment: FlightModeSegment): number {
		if (totalDuration === 0) return 1;
		return segment.duration_s / totalDuration;
	}

	function shouldShowLabel(segment: FlightModeSegment): boolean {
		return totalDuration === 0 || segment.duration_s / totalDuration > 0.06;
	}
</script>

<div class="flex h-8 shrink-0">
	{#each segments as segment (segment.start_us)}
		<div
			class="flex items-center justify-center overflow-hidden"
			style="flex: {flexValue(segment)}; background-color: {getModeColor(segment.mode_id)};"
		>
			{#if shouldShowLabel(segment)}
				<span class="text-[10px] font-semibold text-white/90 truncate px-1">
					{getModeName(segment.mode_id)}
				</span>
			{/if}
		</div>
	{/each}
</div>
