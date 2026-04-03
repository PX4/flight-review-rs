<script lang="ts">
	import ProcessTable from './ProcessTable.svelte';
	import PerfCounterTable from './PerfCounterTable.svelte';

	let { multiInfo }: { multiInfo: Record<string, string[]> } = $props();

	function getText(key: string): string | null {
		const lines = multiInfo[key];
		if (!lines || lines.length === 0) return null;
		return lines.join('\n');
	}

	const consoleOutput = $derived(getText('boot_console_output'));
	const perfTopPre = $derived(getText('perf_top_preflight'));
	const perfTopPost = $derived(getText('perf_top_postflight'));
	const perfCounterPre = $derived(getText('perf_counter_preflight'));
	const perfCounterPost = $derived(getText('perf_counter_postflight'));
</script>

<div class="space-y-4">
	<!-- Console Output -->
	<details open class="rounded-lg bg-white ring-1 ring-gray-200">
		<summary class="cursor-pointer select-none px-4 py-3 text-sm font-semibold text-gray-900 hover:bg-gray-50">
			Console Output
		</summary>
		<div class="border-t border-gray-200 px-4 py-3">
			{#if consoleOutput}
				<pre class="max-h-96 overflow-auto rounded-md bg-gray-50 p-3 font-mono text-xs text-gray-800 ring-1 ring-inset ring-gray-200">{consoleOutput}</pre>
			{:else}
				<p class="text-sm text-gray-400 italic">(Not recorded)</p>
			{/if}
		</div>
	</details>

	<!-- Processes -->
	<details open class="rounded-lg bg-white ring-1 ring-gray-200">
		<summary class="cursor-pointer select-none px-4 py-3 text-sm font-semibold text-gray-900 hover:bg-gray-50">
			Processes
		</summary>
		<div class="border-t border-gray-200 px-4 py-3">
			{#if perfTopPre || perfTopPost}
				<div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
					<div>
						<h4 class="mb-2 text-xs font-medium text-gray-500 uppercase tracking-wide">Pre-flight</h4>
						{#if perfTopPre}
							<ProcessTable text={perfTopPre} />
						{:else}
							<p class="text-sm text-gray-400 italic">(Not recorded)</p>
						{/if}
					</div>
					<div>
						<h4 class="mb-2 text-xs font-medium text-gray-500 uppercase tracking-wide">Post-flight</h4>
						{#if perfTopPost}
							<ProcessTable text={perfTopPost} />
						{:else}
							<p class="text-sm text-gray-400 italic">(Not recorded)</p>
						{/if}
					</div>
				</div>
			{:else}
				<p class="text-sm text-gray-400 italic">(Not recorded)</p>
			{/if}
		</div>
	</details>

	<!-- Performance Counters -->
	<details open class="rounded-lg bg-white ring-1 ring-gray-200">
		<summary class="cursor-pointer select-none px-4 py-3 text-sm font-semibold text-gray-900 hover:bg-gray-50">
			Performance Counters
		</summary>
		<div class="border-t border-gray-200 px-4 py-3">
			{#if perfCounterPre || perfCounterPost}
				<div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
					<div>
						<h4 class="mb-2 text-xs font-medium text-gray-500 uppercase tracking-wide">Pre-flight</h4>
						{#if perfCounterPre}
							<PerfCounterTable text={perfCounterPre} />
						{:else}
							<p class="text-sm text-gray-400 italic">(Not recorded)</p>
						{/if}
					</div>
					<div>
						<h4 class="mb-2 text-xs font-medium text-gray-500 uppercase tracking-wide">Post-flight</h4>
						{#if perfCounterPost}
							<PerfCounterTable text={perfCounterPost} />
						{:else}
							<p class="text-sm text-gray-400 italic">(Not recorded)</p>
						{/if}
					</div>
				</div>
			{:else}
				<p class="text-sm text-gray-400 italic">(Not recorded)</p>
			{/if}
		</div>
	</details>
</div>
