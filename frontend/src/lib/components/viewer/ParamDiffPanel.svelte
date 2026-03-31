<script lang="ts">
	import type { ParamDiff, ChangedParam } from '$lib/types';

	let { diffs, changedParams }: { diffs: ParamDiff[]; changedParams: ChangedParam[] } = $props();

	type SortKey = 'name' | 'value' | 'default' | 'delta';
	type SortDir = 'asc' | 'desc';

	let sortKey = $state<SortKey>('name');
	let sortDir = $state<SortDir>('asc');
	let searchText = $state('');
	let showOnlyInFlight = $state(false);
	let copiedParam = $state<string | null>(null);

	const inFlightNames = $derived(new Set(changedParams.filter((c) => c.in_flight).map((c) => c.name)));

	function getDeltaPct(diff: ParamDiff): number {
		if (diff.default === 0) return diff.value === 0 ? 0 : 100;
		return Math.abs((diff.value - diff.default) / diff.default) * 100;
	}

	function getDeltaColor(pct: number): string {
		if (pct > 50) return 'text-red-600';
		if (pct > 20) return 'text-orange-500';
		return 'text-green-600';
	}

	let filteredDiffs = $derived.by(() => {
		const query = searchText.toLowerCase();
		let result = diffs;

		if (showOnlyInFlight) {
			result = result.filter((d) => inFlightNames.has(d.name));
		}

		if (query) {
			result = result.filter((d) => d.name.toLowerCase().includes(query));
		}

		const dir = sortDir === 'asc' ? 1 : -1;
		result = [...result].sort((a, b) => {
			switch (sortKey) {
				case 'name':
					return dir * a.name.localeCompare(b.name);
				case 'value':
					return dir * (a.value - b.value);
				case 'default':
					return dir * (a.default - b.default);
				case 'delta':
					return dir * (getDeltaPct(a) - getDeltaPct(b));
				default:
					return 0;
			}
		});

		return result;
	});

	function toggleSort(key: SortKey) {
		if (sortKey === key) {
			sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		} else {
			sortKey = key;
			sortDir = 'asc';
		}
	}

	function sortArrow(key: SortKey): string {
		if (sortKey !== key) return '';
		return sortDir === 'asc' ? ' \u2191' : ' \u2193';
	}

	async function copyParamName(name: string) {
		try {
			await navigator.clipboard.writeText(name);
			copiedParam = name;
			setTimeout(() => {
				copiedParam = null;
			}, 1500);
		} catch {
			// clipboard not available
		}
	}

	function formatNumber(n: number): string {
		if (Number.isInteger(n)) return n.toString();
		return n.toPrecision(6);
	}
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 overflow-hidden flex flex-col h-full">
	<!-- Controls -->
	<div class="border-b border-gray-200 px-4 py-2 flex flex-wrap items-center gap-2">
		<input
			type="text"
			placeholder="Search parameters..."
			class="rounded-md border-0 bg-gray-50 px-3 py-1 text-sm text-gray-900 ring-1 ring-inset ring-gray-200 placeholder:text-gray-400 focus:ring-2 focus:ring-indigo-500 w-56"
			bind:value={searchText}
		/>

		<label class="ml-auto flex items-center gap-1.5 text-xs text-gray-600 cursor-pointer">
			<input
				type="checkbox"
				class="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
				bind:checked={showOnlyInFlight}
			/>
			Show only in-flight changes
		</label>
	</div>

	<!-- Count -->
	<div class="px-4 py-1 text-xs text-gray-400 border-b border-gray-100">
		{filteredDiffs.length} of {diffs.length} non-default parameters
	</div>

	<!-- Table -->
	<div class="flex-1 overflow-y-auto">
		<table class="min-w-full text-sm">
			<thead class="sticky top-0 bg-gray-50 border-b border-gray-200">
				<tr>
					<th class="text-left px-4 py-2 font-medium text-gray-700 cursor-pointer select-none" onclick={() => toggleSort('name')}>
						Parameter{sortArrow('name')}
					</th>
					<th class="text-right px-4 py-2 font-medium text-gray-700 cursor-pointer select-none" onclick={() => toggleSort('value')}>
						Current{sortArrow('value')}
					</th>
					<th class="text-right px-4 py-2 font-medium text-gray-700 cursor-pointer select-none" onclick={() => toggleSort('default')}>
						Default{sortArrow('default')}
					</th>
					<th class="text-right px-4 py-2 font-medium text-gray-700 cursor-pointer select-none" onclick={() => toggleSort('delta')}>
						Delta{sortArrow('delta')}
					</th>
				</tr>
			</thead>
			<tbody class="divide-y divide-gray-100">
				{#each filteredDiffs as diff (diff.name)}
					{@const pct = getDeltaPct(diff)}
					{@const isInFlight = inFlightNames.has(diff.name)}
					<tr class="hover:bg-gray-50">
						<td class="px-4 py-1.5 font-mono text-xs">
							<button
								class="text-left hover:text-indigo-600 cursor-pointer inline-flex items-center gap-1"
								onclick={() => copyParamName(diff.name)}
								title="Click to copy"
							>
								{#if isInFlight}
									<svg class="size-3.5 text-orange-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
										<path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
									</svg>
								{/if}
								<span>{diff.name}</span>
								{#if copiedParam === diff.name}
									<span class="text-[10px] text-green-600 ml-1">copied</span>
								{/if}
							</button>
						</td>
						<td class="px-4 py-1.5 text-right font-mono text-xs text-gray-900">
							{formatNumber(diff.value)}
						</td>
						<td class="px-4 py-1.5 text-right font-mono text-xs text-gray-500">
							{formatNumber(diff.default)}
						</td>
						<td class="px-4 py-1.5 text-right font-mono text-xs {getDeltaColor(pct)}">
							{pct.toFixed(1)}%
						</td>
					</tr>
				{/each}
				{#if filteredDiffs.length === 0}
					<tr>
						<td colspan="4" class="px-4 py-8 text-center text-sm text-gray-400">
							No matching parameters
						</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>
