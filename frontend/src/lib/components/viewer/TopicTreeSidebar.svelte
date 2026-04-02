<script lang="ts">
	import { getContext } from 'svelte';
	import type { FlightMetadata, PlotConfig, TopicInfo } from '$lib/types';
	import { activePlots, plottedFields } from '$lib/stores/logViewer';
	import { initDuckDB, LogSession } from '$lib/utils/duckdb';

	let { metadata } = $props<{ metadata: FlightMetadata }>();

	const ctx = getContext<{ logId: string }>('log-viewer');

	let searchQuery = $state('');
	let expandedTopics = $state<Set<string>>(new Set());

	// Cache of fetched field schemas per topic key ("topic_multiId")
	let topicFields = $state<Map<string, { name: string; type: string }[]>>(new Map());
	let loadingTopics = $state<Set<string>>(new Set());
	let fieldErrors = $state<Map<string, string>>(new Map());

	const PLOT_COLORS = ['#818cf8', '#fbbf24', '#34d399', '#f87171', '#a78bfa', '#fb923c', '#38bdf8', '#e879f9'];

	// Module-level session cache (shared with PlotStrip)
	const sessionCache = (globalThis as any).__plotSessionCache ??= new Map<string, LogSession>();

	async function getSession(): Promise<LogSession> {
		const logId = ctx.logId;
		if (sessionCache.has(logId)) return sessionCache.get(logId)!;
		const db = await initDuckDB();
		const session = new LogSession(db, logId);
		sessionCache.set(logId, session);
		return session;
	}

	const sortedTopics: [string, TopicInfo][] = $derived(
		(Object.entries(metadata.topics) as [string, TopicInfo][])
			.filter(([name]) => name.toLowerCase().includes(searchQuery.toLowerCase()))
			.sort(([a], [b]) => a.localeCompare(b))
	);

	function topicKey(name: string, multiId: number): string {
		return multiId > 0 ? `${name}_${multiId}` : name;
	}

	function toggleTopic(name: string) {
		const next = new Set(expandedTopics);
		if (next.has(name)) {
			next.delete(name);
		} else {
			next.add(name);
			// Kick off async field fetch (don't block UI)
			const info = metadata.topics[name];
			const key = topicKey(name, info.multi_id);
			if (!topicFields.has(key) && !loadingTopics.has(key)) {
				fetchFields(name, key, info.multi_id);
			}
		}
		expandedTopics = next;
	}

	async function fetchFields(topic: string, key: string, multiId: number) {
		loadingTopics = new Set([...loadingTopics, key]);
		try {
			const session = await getSession();
			const schema = await session.getTopicSchema(topic, multiId);
			topicFields = new Map([...topicFields, [key, schema]]);
		} catch (e) {
			const msg = e instanceof Error ? e.message : 'Failed to load fields';
			fieldErrors = new Map([...fieldErrors, [key, msg]]);
			console.error(`Failed to fetch schema for ${topic}:`, e);
		} finally {
			const updated = new Set(loadingTopics);
			updated.delete(key);
			loadingTopics = updated;
		}
	}

	function isFieldPlotted(topic: string, field: string): boolean {
		return $plottedFields.get(topic)?.has(field) ?? false;
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
			{@const key = topicKey(topicName, topicInfo.multi_id)}
			{@const fields = topicFields.get(key)}
			{@const isLoading = loadingTopics.has(key)}
			{@const fieldError = fieldErrors.get(key)}
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
					{#if topicInfo.multi_id > 0}
						<span class="text-xs text-gray-400">#{topicInfo.multi_id}</span>
					{/if}
					<span class="ml-auto text-xs text-gray-500 bg-gray-100 rounded-full px-2 py-0.5">{topicInfo.message_count}</span>
				</button>
				{#if isExpanded}
					<div class="ml-6 space-y-0.5 mt-1">
						{#if isLoading}
							<p class="px-2 py-1 text-xs text-gray-400 flex items-center gap-1.5">
								<svg class="size-3 animate-spin" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
									<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
									<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
								</svg>
								Loading fields...
							</p>
						{:else if fieldError}
							<p class="px-2 py-1 text-xs text-red-400">{fieldError}</p>
						{:else if fields && fields.length > 0}
							{#each fields as fieldInfo}
								{@const plotted = isFieldPlotted(topicName, fieldInfo.name)}
								<button
									class="flex items-center gap-2 w-full rounded px-2 py-1 text-xs hover:bg-gray-50 {plotted ? 'text-indigo-600 font-medium bg-indigo-50' : 'text-gray-600'}"
									onclick={() => toggleField(topicName, fieldInfo.name, topicInfo.multi_id)}
								>
									<span class="size-3 shrink-0 rounded border {plotted ? 'border-indigo-500 bg-indigo-500' : 'border-gray-300'}">
										{#if plotted}
											<svg class="size-3 text-white" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2">
												<path d="M2.5 6l2.5 2.5 4.5-4.5" />
											</svg>
										{/if}
									</span>
									<span class="truncate">{fieldInfo.name}</span>
									<span class="ml-auto text-[10px] text-gray-400 shrink-0">{fieldInfo.type}</span>
								</button>
							{/each}
						{:else if fields}
							<p class="px-2 py-1 text-xs text-gray-400 italic">No numeric fields</p>
						{/if}
					</div>
				{/if}
			</div>
		{/each}
		{#if sortedTopics.length === 0}
			<p class="text-sm text-gray-500 px-2 py-4">No topics match "{searchQuery}"</p>
		{/if}
	</div>
</div>
