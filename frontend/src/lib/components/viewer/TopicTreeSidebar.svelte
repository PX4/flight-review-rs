<script lang="ts">
	import type { FlightMetadata, PlotConfig, TopicInfo } from '$lib/types';
	import { activePlots, plottedFields } from '$lib/stores/logViewer';

	let { metadata } = $props<{ metadata: FlightMetadata }>();

	let searchQuery = $state('');
	let expandedTopics = $state<Set<string>>(new Set());

	const PLOT_COLORS = ['#818cf8', '#fbbf24', '#34d399', '#f87171', '#a78bfa', '#fb923c', '#38bdf8', '#e879f9'];

	const sortedTopics: [string, TopicInfo][] = $derived(
		(Object.entries(metadata.topics) as [string, TopicInfo][])
			.filter(([name]) => name.toLowerCase().includes(searchQuery.toLowerCase()))
			.sort(([a], [b]) => a.localeCompare(b))
	);

	function toggleTopic(name: string) {
		const next = new Set(expandedTopics);
		if (next.has(name)) {
			next.delete(name);
		} else {
			next.add(name);
		}
		expandedTopics = next;
	}

	function getTopicFields(topicName: string): string[] {
		// We don't have field-level info in metadata.topics, so for MVP
		// we show a placeholder list based on known common fields.
		// In the real implementation, field names come from Parquet schema.
		return [];
	}

	function isFieldPlotted(topic: string, field: string): boolean {
		let plotted: Map<string, Set<string>> = new Map();
		plottedFields.subscribe((v) => (plotted = v))();
		return plotted.get(topic)?.has(field) ?? false;
	}

	function toggleField(topic: string, field: string, multiId: number) {
		activePlots.update((plots) => {
			const existing = plots.find((p) => p.topic === topic);
			if (existing) {
				const hasField = existing.fields.includes(field);
				if (hasField) {
					const newFields = existing.fields.filter((f) => f !== field);
					if (newFields.length === 0) {
						return plots.filter((p) => p.topic !== topic);
					}
					return plots.map((p) =>
						p.topic === topic
							? { ...p, fields: newFields, colors: p.colors.slice(0, newFields.length) }
							: p
					);
				} else {
					const colorIdx = existing.fields.length % PLOT_COLORS.length;
					return plots.map((p) =>
						p.topic === topic
							? { ...p, fields: [...p.fields, field], colors: [...p.colors, PLOT_COLORS[colorIdx]] }
							: p
					);
				}
			} else {
				const newPlot: PlotConfig = {
					id: `${topic}_${multiId}`,
					topic,
					multiId,
					fields: [field],
					yLabel: topic,
					colors: [PLOT_COLORS[0]],
				};
				return [...plots, newPlot];
			}
		});
	}
</script>

<div class="flex-1 overflow-y-auto px-4 py-4">
	<!-- Search -->
	<div class="mb-4">
		<input
			type="text"
			placeholder="Search topics..."
			bind:value={searchQuery}
			class="block w-full rounded-md bg-white px-3 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
		/>
	</div>

	<!-- Topic tree -->
	<div class="space-y-1">
		{#each sortedTopics as [topicName, topicInfo]}
			{@const isExpanded = expandedTopics.has(topicName)}
			<div>
				<button
					class="flex items-center gap-2 w-full rounded-md px-2 py-1.5 text-sm hover:bg-gray-50 {isExpanded ? 'text-gray-900' : 'text-gray-700 hover:text-gray-900'}"
					onclick={() => toggleTopic(topicName)}
				>
					{#if isExpanded}
						<svg class="size-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
							<path stroke-linecap="round" stroke-linejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5" />
						</svg>
					{:else}
						<svg class="size-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
							<path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
						</svg>
					{/if}
					<span class="font-medium">{topicName}</span>
					<span class="ml-auto text-xs text-gray-500 bg-gray-100 rounded-full px-2 py-0.5">{topicInfo.message_count}</span>
				</button>
				{#if isExpanded}
					<div class="ml-6 space-y-0.5 mt-1">
						<p class="px-2 py-1 text-xs text-gray-400 italic">
							Field details available after Parquet integration
						</p>
					</div>
				{/if}
			</div>
		{/each}
		{#if sortedTopics.length === 0}
			<p class="text-sm text-gray-500 px-2 py-4">No topics match "{searchQuery}"</p>
		{/if}
	</div>
</div>
