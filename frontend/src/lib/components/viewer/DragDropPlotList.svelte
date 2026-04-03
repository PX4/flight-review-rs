<script lang="ts">
	import type { PlotConfig, FlightMetadata } from '$lib/types';
	import { reorderPlots } from '$lib/stores/logViewer';
	import PlotStrip from './PlotStrip.svelte';

	let { plots, logId, metadata } = $props<{
		plots: PlotConfig[];
		logId: string;
		metadata: FlightMetadata;
	}>();

	let dragFromIndex = $state<number | null>(null);
	let dropTargetIndex = $state<number | null>(null);

	function handleDragStart(index: number) {
		return (e: DragEvent) => {
			dragFromIndex = index;
			if (e.dataTransfer) {
				e.dataTransfer.effectAllowed = 'move';
				e.dataTransfer.setData('text/plain', String(index));
			}
		};
	}

	function handleDragEnd() {
		dragFromIndex = null;
		dropTargetIndex = null;
	}

	function handleDragOver(index: number) {
		return (e: DragEvent) => {
			e.preventDefault();
			if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
			if (dragFromIndex === null) return;
			dropTargetIndex = index;
		};
	}

	function handleDrop(index: number) {
		return (e: DragEvent) => {
			e.preventDefault();
			if (dragFromIndex !== null && dragFromIndex !== index) {
				reorderPlots(dragFromIndex, index);
			}
			dragFromIndex = null;
			dropTargetIndex = null;
		};
	}

	function handleDragLeave() {
		dropTargetIndex = null;
	}

	function moveUp(index: number) {
		if (index > 0) reorderPlots(index, index - 1);
	}

	function moveDown(index: number) {
		if (index < plots.length - 1) reorderPlots(index, index + 1);
	}
</script>

<div class="flex flex-col gap-4">
	{#each plots as plot, i (plot.id)}
		<div
			class="relative transition-opacity duration-150"
			class:opacity-50={dragFromIndex === i}
			ondragover={handleDragOver(i)}
			ondrop={handleDrop(i)}
			ondragleave={handleDragLeave}
		>
			{#if dropTargetIndex === i && dragFromIndex !== null && dragFromIndex !== i}
				<div class="absolute -top-px left-0 right-0 h-0.5 bg-blue-500 z-10 rounded-full"></div>
			{/if}
			<PlotStrip
				config={plot}
				{logId}
				{metadata}
				index={i}
				totalCount={plots.length}
				onMoveUp={() => moveUp(i)}
				onMoveDown={() => moveDown(i)}
				onDragStart={handleDragStart(i)}
				onDragEnd={handleDragEnd}
			/>
		</div>
	{/each}
</div>
