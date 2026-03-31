<script lang="ts">
	import { goto } from '$app/navigation';
	import type { LogRecord } from '$lib/types';
	import { formatDuration, formatFileSize, formatRelativeTime } from '$lib/utils/formatters';
	import { getHardwareName } from '$lib/utils/hardwareNames';
	import FlightThumbnail from './FlightThumbnail.svelte';

	let { logs, sortField, sortDir, onSort }: {
		logs: LogRecord[];
		sortField: string | null;
		sortDir: 'asc' | 'desc';
		onSort: (field: string) => void;
	} = $props();

	interface Column {
		key: string;
		label: string;
		sortable: boolean;
		hiddenMobile?: boolean;
	}

	const columns: Column[] = [
		{ key: 'created_at', label: 'Date', sortable: true },
		{ key: 'sys_name', label: 'Vehicle', sortable: true },
		{ key: 'ver_hw', label: 'Hardware', sortable: true },
		{ key: 'flight_duration_s', label: 'Duration', sortable: true },
		{ key: 'topic_count', label: 'Topics', sortable: true, hiddenMobile: true },
		{ key: 'file_size', label: 'Size', sortable: true, hiddenMobile: true },
		{ key: 'location_name', label: 'Location', sortable: false, hiddenMobile: true },
	];

	function sortIndicator(key: string): string {
		if (sortField !== key) return '';
		return sortDir === 'asc' ? ' \u2191' : ' \u2193';
	}

	function handleRowClick(id: string) {
		goto(`/log/${id}`);
	}

	function cellValue(log: LogRecord, key: string): string {
		switch (key) {
			case 'created_at':
				return formatRelativeTime(log.created_at);
			case 'sys_name':
				return log.sys_name ?? '\u2014';
			case 'ver_hw':
				return getHardwareName(log.ver_hw);
			case 'flight_duration_s':
				return formatDuration(log.flight_duration_s);
			case 'topic_count':
				return String(log.topic_count);
			case 'file_size':
				return formatFileSize(log.file_size);
			case 'location_name':
				return log.location_name ?? '\u2014';
			default:
				return '\u2014';
		}
	}
</script>

<div class="flow-root">
	<div class="overflow-x-auto">
		<div class="inline-block min-w-full align-middle">
			<table class="min-w-full divide-y divide-gray-200">
				<thead>
					<tr>
						<th scope="col" class="hidden lg:table-cell py-3.5 pl-3 pr-2 text-left text-sm font-semibold text-gray-900">
							Track
						</th>
						{#each columns as col}
							<th
								scope="col"
								class="px-2 sm:px-3 py-3.5 text-left text-sm font-semibold text-gray-900 {col.sortable ? 'cursor-pointer select-none hover:text-indigo-600' : ''} {col.hiddenMobile ? 'hidden sm:table-cell' : ''}"
								onclick={() => col.sortable && onSort(col.key)}
							>
								{col.label}{sortIndicator(col.key)}
							</th>
						{/each}
					</tr>
				</thead>
				<tbody class="divide-y divide-gray-100">
					{#each logs as log (log.id)}
						<tr
							class="hover:bg-gray-50 cursor-pointer"
							onclick={() => handleRowClick(log.id)}
						>
							<td class="hidden lg:table-cell py-2 pl-3 pr-2">
								<FlightThumbnail logId={log.id} width={120} height={72} />
							</td>
							{#each columns as col}
								<td
									class="whitespace-nowrap px-2 sm:px-3 py-3 sm:py-4 text-sm {col.key === 'sys_name' ? 'font-medium text-gray-900' : 'text-gray-500'} {col.hiddenMobile ? 'hidden sm:table-cell' : ''}"
								>
									{cellValue(log, col.key)}
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</div>
</div>
