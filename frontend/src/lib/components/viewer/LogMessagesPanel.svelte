<script lang="ts">
	import type { LogEntry } from '$lib/types';
	import { cursorTimestamp } from '$lib/stores/plotSync';

	let { messages, flightStartUs }: { messages: LogEntry[]; flightStartUs: number } = $props();

	const ROW_HEIGHT = 28;
	const LEVELS = ['ERROR', 'WARNING', 'INFO', 'DEBUG'] as const;
	type Level = (typeof LEVELS)[number];

	const LEVEL_COLORS: Record<Level, { bg: string; text: string; badge: string }> = {
		ERROR: { bg: 'bg-red-50', text: 'text-red-700', badge: 'bg-red-100 text-red-700' },
		WARNING: { bg: 'bg-orange-50', text: 'text-orange-700', badge: 'bg-orange-100 text-orange-700' },
		INFO: { bg: 'bg-white', text: 'text-gray-700', badge: 'bg-gray-100 text-gray-700' },
		DEBUG: { bg: 'bg-white', text: 'text-gray-400', badge: 'bg-gray-50 text-gray-400' },
	};

	let enabledLevels = $state<Set<Level>>(new Set(['ERROR', 'WARNING', 'INFO']));
	let searchText = $state('');
	let scrollTop = $state(0);
	let containerHeight = $state(400);

	function toggleLevel(level: Level) {
		const next = new Set(enabledLevels);
		if (next.has(level)) {
			next.delete(level);
		} else {
			next.add(level);
		}
		enabledLevels = next;
	}

	function normalizeLevel(level: string): Level {
		const upper = level.toUpperCase();
		if (upper === 'ERROR' || upper === 'ERR') return 'ERROR';
		if (upper === 'WARNING' || upper === 'WARN') return 'WARNING';
		if (upper === 'INFO') return 'INFO';
		return 'DEBUG';
	}

	let filteredMessages = $derived.by(() => {
		const query = searchText.toLowerCase();
		return messages.filter((m) => {
			const level = normalizeLevel(m.level);
			if (!enabledLevels.has(level)) return false;
			if (query && !m.message.toLowerCase().includes(query)) return false;
			return true;
		});
	});

	let totalHeight = $derived(filteredMessages.length * ROW_HEIGHT);
	let startIndex = $derived(Math.floor(scrollTop / ROW_HEIGHT));
	let visibleCount = $derived(Math.ceil(containerHeight / ROW_HEIGHT) + 1);
	let endIndex = $derived(Math.min(startIndex + visibleCount, filteredMessages.length));
	let visibleMessages = $derived(filteredMessages.slice(startIndex, endIndex));
	let offsetY = $derived(startIndex * ROW_HEIGHT);

	function formatTimestamp(timestampUs: number): string {
		const relativeUs = timestampUs - flightStartUs;
		const totalMs = Math.max(0, relativeUs / 1000);
		const ms = Math.floor(totalMs % 1000);
		const totalSec = Math.floor(totalMs / 1000);
		const sec = totalSec % 60;
		const totalMin = Math.floor(totalSec / 60);
		const min = totalMin % 60;
		const hr = Math.floor(totalMin / 60);
		return `${String(hr).padStart(2, '0')}:${String(min).padStart(2, '0')}:${String(sec).padStart(2, '0')}.${String(ms).padStart(3, '0')}`;
	}

	function handleMessageClick(timestampUs: number) {
		cursorTimestamp.set(timestampUs);
	}

	function handleScroll(e: Event) {
		const target = e.target as HTMLElement;
		scrollTop = target.scrollTop;
	}
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 overflow-hidden flex flex-col lg:h-full">
	<!-- Controls -->
	<div class="border-b border-gray-200 px-3 sm:px-4 py-2 flex flex-wrap items-center gap-2">
		<!-- Level filters -->
		{#each LEVELS as level}
			{@const colors = LEVEL_COLORS[level]}
			<button
				class="inline-flex items-center rounded-md px-2 py-1 text-xs font-medium ring-1 ring-inset transition-opacity
					{enabledLevels.has(level) ? colors.badge + ' ring-current/10' : 'bg-gray-50 text-gray-300 ring-gray-200'}"
				onclick={() => toggleLevel(level)}
			>
				{level}
			</button>
		{/each}

		<!-- Search -->
		<div class="w-full sm:w-auto sm:ml-auto flex-shrink-0">
			<input
				type="text"
				placeholder="Search messages..."
				class="rounded-md border-0 bg-gray-50 px-3 py-1 text-sm text-gray-900 ring-1 ring-inset ring-gray-200 placeholder:text-gray-400 focus:ring-2 focus:ring-indigo-500 w-full sm:w-48"
				bind:value={searchText}
			/>
		</div>
	</div>

	<!-- Message count -->
	<div class="px-3 sm:px-4 py-1 text-xs text-gray-400 border-b border-gray-100">
		{filteredMessages.length} of {messages.length} messages
	</div>

	<!-- Virtual scrolled messages -->
	<div
		class="flex-1 overflow-y-auto font-mono text-xs"
		onscroll={handleScroll}
		bind:clientHeight={containerHeight}
	>
		<div style="height: {totalHeight}px; position: relative;">
			<div style="transform: translateY({offsetY}px);">
				{#each visibleMessages as msg, i (startIndex + i)}
					{@const level = normalizeLevel(msg.level)}
					{@const colors = LEVEL_COLORS[level]}
					<button
						class="w-full text-left flex items-center gap-1.5 sm:gap-2 px-3 sm:px-4 h-7 hover:bg-gray-50 cursor-pointer {colors.bg}"
						onclick={() => handleMessageClick(msg.timestamp_us)}
					>
						<span class="text-gray-400 flex-shrink-0">[{formatTimestamp(msg.timestamp_us)}]</span>
						<span class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-semibold flex-shrink-0 {colors.badge}">
							{level}
						</span>
						<span class="truncate {colors.text}">{msg.message}</span>
					</button>
				{/each}
			</div>
		</div>
	</div>
</div>
