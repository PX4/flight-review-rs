<script lang="ts">
	interface PerfEntry {
		name: string;
		type: 'cycle' | 'elapsed' | 'event';
		count: number;
		elapsed_us: number | null;
		avg_us: number | null;
		min_us: number | null;
		max_us: number | null;
		rms_us: number | null;
	}

	type SortKey = 'name' | 'count' | 'avg_us' | 'min_us' | 'max_us' | 'rms_us';

	let { text }: { text: string } = $props();

	let sortKey: SortKey = $state('count');
	let sortAsc: boolean = $state(false);

	// Regex for elapsed/cycle lines:
	// name: [cycle: |cycle time: ]N events, Xus elapsed, Y.YYus avg, min Zus max Wus R.RRus rms
	const FULL_RE =
		/^(.+?):\s+(?:(cycle(?:\s+time)?)\s*:\s+)?(\d+)\s+events,\s+([\d.]+)us\s+elapsed,\s+([\d.]+|inf)us\s+avg,\s+min\s+([\d.]+|inf)us\s+max\s+([\d.]+|inf)us\s+([\d.]+|inf)us\s+rms\s*$/;

	// Simple event counter: name: N events
	const EVENT_RE = /^(.+?):\s+(\d+)\s+events\s*$/;

	function parseNum(s: string): number | null {
		if (s === 'inf') return null;
		const n = parseFloat(s);
		return isNaN(n) ? null : n;
	}

	function parseLine(line: string): PerfEntry | null {
		const trimmed = line.trim();
		if (!trimmed) return null;

		let m = FULL_RE.exec(trimmed);
		if (m) {
			return {
				name: m[1].trim(),
				type: m[2] ? 'cycle' : 'elapsed',
				count: parseInt(m[3], 10),
				elapsed_us: parseNum(m[4]),
				avg_us: parseNum(m[5]),
				min_us: parseNum(m[6]),
				max_us: parseNum(m[7]),
				rms_us: parseNum(m[8])
			};
		}

		m = EVENT_RE.exec(trimmed);
		if (m) {
			return {
				name: m[1].trim(),
				type: 'event',
				count: parseInt(m[2], 10),
				elapsed_us: null,
				avg_us: null,
				min_us: null,
				max_us: null,
				rms_us: null
			};
		}

		return null;
	}

	const entries = $derived<PerfEntry[]>(
		text
			.split('\n')
			.map(parseLine)
			.filter((e): e is PerfEntry => e !== null)
	);

	const maxAvg = $derived(
		Math.max(...entries.map((e) => e.avg_us ?? 0), 1)
	);

	const sortedEntries = $derived.by<PerfEntry[]>(() => {
		const active = entries.filter((e) => e.count > 0);
		const inactive = entries.filter((e) => e.count === 0);

		const compare = (a: PerfEntry, b: PerfEntry): number => {
			let va: string | number;
			let vb: string | number;

			if (sortKey === 'name') {
				va = a.name.toLowerCase();
				vb = b.name.toLowerCase();
			} else {
				va = a[sortKey] ?? -1;
				vb = b[sortKey] ?? -1;
			}

			if (va < vb) return sortAsc ? -1 : 1;
			if (va > vb) return sortAsc ? 1 : -1;
			return 0;
		};

		return [...active.sort(compare), ...inactive.sort(compare)];
	});

	function handleSort(key: SortKey) {
		if (sortKey === key) {
			sortAsc = !sortAsc;
		} else {
			sortKey = key;
			sortAsc = key === 'name';
		}
	}

	function fmt(n: number | null): string {
		if (n === null) return '\u2014';
		return n.toLocaleString(undefined, { maximumFractionDigits: 2 });
	}

	function fmtInt(n: number): string {
		return n.toLocaleString();
	}

	function barColor(avg: number): string {
		if (avg < 100) return 'bg-green-500';
		if (avg <= 500) return 'bg-yellow-500';
		return 'bg-red-500';
	}

	function sortIndicator(key: SortKey): string {
		if (sortKey !== key) return '';
		return sortAsc ? ' \u25B2' : ' \u25BC';
	}

	function hasJitter(entry: PerfEntry): boolean {
		if (entry.avg_us === null || entry.max_us === null || entry.avg_us === 0) return false;
		return entry.max_us > 10 * entry.avg_us;
	}
</script>

{#if entries.length === 0}
	<p class="text-sm text-gray-400 italic">No perf counter data</p>
{:else}
	<div class="overflow-auto max-h-[32rem]">
		<table class="w-full text-xs text-left border-collapse">
			<thead class="sticky top-0 bg-gray-100 z-10">
				<tr>
					<th
						class="px-2 py-1.5 font-medium text-gray-600 cursor-pointer select-none hover:text-gray-900 whitespace-nowrap"
						onclick={() => handleSort('name')}
					>
						Name{sortIndicator('name')}
					</th>
					<th
						class="px-2 py-1.5 font-medium text-gray-600 cursor-pointer select-none hover:text-gray-900 text-right whitespace-nowrap"
						onclick={() => handleSort('count')}
					>
						Count{sortIndicator('count')}
					</th>
					<th
						class="px-2 py-1.5 font-medium text-gray-600 cursor-pointer select-none hover:text-gray-900 whitespace-nowrap min-w-[10rem]"
						onclick={() => handleSort('avg_us')}
					>
						Avg (&micro;s){sortIndicator('avg_us')}
					</th>
					<th
						class="px-2 py-1.5 font-medium text-gray-600 cursor-pointer select-none hover:text-gray-900 text-right whitespace-nowrap"
						onclick={() => handleSort('min_us')}
					>
						Min (&micro;s){sortIndicator('min_us')}
					</th>
					<th
						class="px-2 py-1.5 font-medium text-gray-600 cursor-pointer select-none hover:text-gray-900 text-right whitespace-nowrap"
						onclick={() => handleSort('max_us')}
					>
						Max (&micro;s){sortIndicator('max_us')}
					</th>
					<th
						class="px-2 py-1.5 font-medium text-gray-600 cursor-pointer select-none hover:text-gray-900 text-right whitespace-nowrap"
						onclick={() => handleSort('rms_us')}
					>
						RMS (&micro;s){sortIndicator('rms_us')}
					</th>
				</tr>
			</thead>
			<tbody>
				{#each sortedEntries as entry, i (entry.name + entry.type + '_' + i)}
					<tr
						class="border-t border-gray-100 hover:bg-gray-50 {entry.count === 0
							? 'opacity-40'
							: ''}"
					>
						<td class="px-2 py-1 font-mono text-gray-900 whitespace-nowrap">
							{entry.name}
							{#if entry.type === 'cycle'}
								<span class="ml-1 rounded bg-blue-100 px-1 py-0.5 text-[10px] font-medium text-blue-700"
									>cycle</span
								>
							{/if}
						</td>
						<td class="px-2 py-1 text-right font-mono tabular-nums text-gray-700">
							{fmtInt(entry.count)}
						</td>
						<td class="px-2 py-1">
							{#if entry.avg_us !== null}
								<div class="flex items-center gap-1.5">
									<span class="font-mono tabular-nums text-gray-700 shrink-0"
										>{fmt(entry.avg_us)}</span
									>
									<div class="flex-1 h-2.5 bg-gray-100 rounded-sm overflow-hidden">
										<div
											class="h-full rounded-sm {barColor(entry.avg_us)}"
											style="width: {Math.max((entry.avg_us / maxAvg) * 100, 0.5)}%"
										></div>
									</div>
								</div>
							{:else}
								<span class="text-gray-300">&mdash;</span>
							{/if}
						</td>
						<td class="px-2 py-1 text-right font-mono tabular-nums text-gray-700">
							{fmt(entry.min_us)}
						</td>
						<td class="px-2 py-1 text-right font-mono tabular-nums text-gray-700 whitespace-nowrap">
							{fmt(entry.max_us)}
							{#if hasJitter(entry)}
								<span class="ml-0.5 text-orange-500" title="High jitter: max > 10x avg"
									>&#9888;</span
								>
							{/if}
						</td>
						<td class="px-2 py-1 text-right font-mono tabular-nums text-gray-700">
							{fmt(entry.rms_us)}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
