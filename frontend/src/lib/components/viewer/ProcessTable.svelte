<script lang="ts">
	interface ProcessRow {
		pid: number;
		command: string;
		cpuMs: number;
		cpuPct: number;
		usedStack: number;
		totalStack: number;
		priority: number;
		basePriority: number;
		state: string;
		fd: number;
	}

	interface Summary {
		processes?: string;
		cpuUsage?: string;
		memory?: string;
		uptime?: string;
	}

	type SortKey = 'pid' | 'command' | 'cpuPct' | 'stack' | 'priority' | 'state';

	let { text }: { text: string } = $props();

	let sortKey: SortKey = $state('cpuPct');
	let sortAsc: boolean = $state(false);

	function parseRows(input: string): ProcessRow[] {
		const lines = input.split('\n');
		const rows: ProcessRow[] = [];

		for (const line of lines) {
			// Skip empty, header, and summary lines
			const trimmed = line.trim();
			if (!trimmed) continue;
			if (trimmed.startsWith('PID')) continue;
			if (trimmed.startsWith('Processes:')) continue;
			if (trimmed.startsWith('CPU usage:')) continue;
			if (trimmed.startsWith('DMA Memory:')) continue;
			if (trimmed.startsWith('Uptime:')) continue;

			// Parse fixed-width process line using regex
			// Format: PID COMMAND CPU(ms) CPU(%) USED/STACK PRIO(BASE) STATE FD
			// Example: "   0 Idle Task                  310295 52.747   248/  512   0 (  0)  READY  3"
			const match = trimmed.match(
				/^(\d+)\s+(.+?)\s{2,}(\d+)\s+([\d.]+)\s+(\d+)\s*\/\s*(\d+)\s+(\d+)\s*\(\s*(\d+)\)\s+(\S+)\s+(\d+)$/
			);

			if (match) {
				rows.push({
					pid: parseInt(match[1], 10),
					command: match[2].trim(),
					cpuMs: parseInt(match[3], 10),
					cpuPct: parseFloat(match[4]),
					usedStack: parseInt(match[5], 10),
					totalStack: parseInt(match[6], 10),
					priority: parseInt(match[7], 10),
					basePriority: parseInt(match[8], 10),
					state: match[9],
					fd: parseInt(match[10], 10)
				});
			}
		}

		return rows;
	}

	function parseSummary(input: string): Summary {
		const summary: Summary = {};
		for (const line of input.split('\n')) {
			const trimmed = line.trim();
			if (trimmed.startsWith('Processes:')) summary.processes = trimmed;
			else if (trimmed.startsWith('CPU usage:')) summary.cpuUsage = trimmed;
			else if (trimmed.startsWith('DMA Memory:')) summary.memory = trimmed;
			else if (trimmed.startsWith('Uptime:')) summary.uptime = trimmed;
		}
		return summary;
	}

	const rows = $derived(parseRows(text));
	const summary = $derived(parseSummary(text));

	const sortedRows = $derived.by(() => {
		const sorted = [...rows];
		sorted.sort((a, b) => {
			let cmp = 0;
			switch (sortKey) {
				case 'pid':
					cmp = a.pid - b.pid;
					break;
				case 'command':
					cmp = a.command.localeCompare(b.command);
					break;
				case 'cpuPct':
					cmp = a.cpuPct - b.cpuPct;
					break;
				case 'stack':
					cmp = stackPct(a) - stackPct(b);
					break;
				case 'priority':
					cmp = a.priority - b.priority;
					break;
				case 'state':
					cmp = a.state.localeCompare(b.state);
					break;
			}
			return sortAsc ? cmp : -cmp;
		});
		return sorted;
	});

	function stackPct(row: ProcessRow): number {
		if (row.totalStack === 0) return 0;
		return (row.usedStack / row.totalStack) * 100;
	}

	function toggleSort(key: SortKey) {
		if (sortKey === key) {
			sortAsc = !sortAsc;
		} else {
			sortKey = key;
			sortAsc = key === 'command' || key === 'state';
		}
	}

	function sortIndicator(key: SortKey): string {
		if (sortKey !== key) return '';
		return sortAsc ? ' \u25B2' : ' \u25BC';
	}

	function cpuBarColor(pct: number): string {
		if (pct > 15) return 'bg-red-400';
		if (pct >= 5) return 'bg-amber-400';
		return 'bg-emerald-400';
	}

	function stackBarColor(pct: number): string {
		if (pct > 80) return 'bg-red-400';
		if (pct >= 60) return 'bg-amber-400';
		return 'bg-emerald-400';
	}

	function isIdle(row: ProcessRow): boolean {
		return row.command.toLowerCase() === 'idle task';
	}

	const columns: { key: SortKey; label: string; align: string }[] = [
		{ key: 'pid', label: 'PID', align: 'text-right' },
		{ key: 'command', label: 'Command', align: 'text-left' },
		{ key: 'cpuPct', label: 'CPU %', align: 'text-right' },
		{ key: 'stack', label: 'Stack', align: 'text-right' },
		{ key: 'priority', label: 'Priority', align: 'text-right' },
		{ key: 'state', label: 'State', align: 'text-left' }
	];

	const hasSummary = $derived(
		summary.processes || summary.cpuUsage || summary.memory || summary.uptime
	);
</script>

<div class="space-y-2">
	{#if sortedRows.length > 0}
		<div class="overflow-x-auto rounded-md ring-1 ring-gray-200">
			<table class="w-full text-xs">
				<thead>
					<tr class="bg-gray-50 text-gray-500">
						{#each columns as col (col.key)}
							<th
								class="cursor-pointer select-none whitespace-nowrap px-2 py-1.5 font-medium {col.align} hover:text-gray-900"
								onclick={() => toggleSort(col.key)}
							>
								{col.label}{sortIndicator(col.key)}
							</th>
						{/each}
					</tr>
				</thead>
				<tbody class="divide-y divide-gray-100">
					{#each sortedRows as row (row.pid)}
						{@const idle = isIdle(row)}
						{@const sPct = stackPct(row)}
						<tr class="hover:bg-gray-50 {idle ? 'opacity-50' : ''}">
							<td class="whitespace-nowrap px-2 py-1 text-right tabular-nums text-gray-500">
								{row.pid}
							</td>
							<td class="whitespace-nowrap px-2 py-1 font-medium text-gray-900">
								{row.command}
							</td>
							<td class="whitespace-nowrap px-2 py-1 text-right">
								<div class="flex items-center justify-end gap-1.5">
									<span class="tabular-nums text-gray-700">{row.cpuPct.toFixed(1)}</span>
									<div class="h-2.5 w-16 rounded-sm bg-gray-100">
										<div
											class="h-full rounded-sm {cpuBarColor(row.cpuPct)}"
											style="width: {Math.min(row.cpuPct, 100)}%"
										></div>
									</div>
								</div>
							</td>
							<td class="whitespace-nowrap px-2 py-1 text-right">
								<div class="flex flex-col items-end gap-0.5">
									<span class="tabular-nums text-gray-700">{row.usedStack}/{row.totalStack}</span>
									<div class="h-1 w-12 rounded-sm bg-gray-100">
										<div
											class="h-full rounded-sm {stackBarColor(sPct)}"
											style="width: {Math.min(sPct, 100)}%"
										></div>
									</div>
								</div>
							</td>
							<td class="whitespace-nowrap px-2 py-1 text-right tabular-nums text-gray-700">
								{row.priority}
								<span class="text-gray-400">({row.basePriority})</span>
							</td>
							<td class="whitespace-nowrap px-2 py-1 text-gray-700">
								{row.state}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else}
		<p class="text-sm text-gray-400 italic">No process data parsed</p>
	{/if}

	{#if hasSummary}
		<div class="flex flex-wrap gap-x-4 gap-y-1 rounded-md bg-gray-50 px-3 py-2 text-xs text-gray-600 ring-1 ring-inset ring-gray-200">
			{#if summary.processes}
				<span>{summary.processes}</span>
			{/if}
			{#if summary.cpuUsage}
				<span>{summary.cpuUsage}</span>
			{/if}
			{#if summary.memory}
				<span>{summary.memory}</span>
			{/if}
			{#if summary.uptime}
				<span>{summary.uptime}</span>
			{/if}
		</div>
	{/if}
</div>
