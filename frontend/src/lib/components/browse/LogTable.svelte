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
		{ key: 'vehicle_type', label: 'Type', sortable: true },
		{ key: 'ver_hw', label: 'Hardware', sortable: true },
		{ key: 'flight_duration_s', label: 'Duration', sortable: true },
		{ key: 'location_name', label: 'Location', sortable: false },
		{ key: 'topic_count', label: 'Topics', sortable: true, hiddenMobile: true },
		{ key: 'file_size', label: 'Size', sortable: true, hiddenMobile: true },
	];

	function sortIndicator(key: string): string {
		if (sortField !== key) return '';
		return sortDir === 'asc' ? ' \u2191' : ' \u2193';
	}

	function handleRowClick(id: string) {
		goto(`/log/${id}`);
	}

	function vehicleIconPath(type: string | null | undefined): string {
		switch (type?.toLowerCase()) {
			case 'multirotor': return '/icons/quadrotor.svg';
			case 'fixed wing': return '/icons/fixedwing.svg';
			case 'vtol': return '/icons/vtol.svg';
			case 'rover': return '/icons/rover.svg';
			case 'boat': return '/icons/submarine.svg';
			case 'submarine': return '/icons/submarine.svg';
			default: return '/icons/unknown.svg';
		}
	}

	function parseLocation(name: string | null | undefined): { city: string | null; country: string | null; flag: string | null } | null {
		if (!name) return null;
		const ccMatch = name.match(/\[([A-Z]{2})\]$/);
		const cc = ccMatch ? ccMatch[1] : null;
		const clean = ccMatch ? name.replace(/\s*\[[A-Z]{2}\]$/, '') : name;
		const parts = clean.split(', ');
		const city = parts.length > 1 ? parts[0] : null;
		const country = parts.length > 1 ? parts.slice(1).join(', ') : parts[0];
		const flag = cc ? String.fromCodePoint(...[...cc].map(c => 0x1F1E6 + c.charCodeAt(0) - 65)) : null;
		return { city, country, flag };
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
						{@const loc = parseLocation(log.location_name)}
						<tr
							class="hover:bg-gray-50 cursor-pointer"
							onclick={() => handleRowClick(log.id)}
						>
							<td class="hidden lg:table-cell py-2 pl-3 pr-2">
								<FlightThumbnail logId={log.id} width={120} height={72} />
							</td>
							{#each columns as col}
								<td
									class="whitespace-nowrap px-2 sm:px-3 py-3 sm:py-4 text-sm text-gray-500 {col.hiddenMobile ? 'hidden sm:table-cell' : ''}"
								>
									{#if col.key === 'vehicle_type'}
										<div class="flex items-center gap-2">
											<img src={vehicleIconPath(log.vehicle_type)} alt="" class="size-6 opacity-60" />
											<span class="font-medium text-gray-900">{log.vehicle_type || log.sys_name || '\u2014'}</span>
										</div>
									{:else if col.key === 'location_name'}
										{#if loc}
											<div class="flex items-center gap-1.5">
												{#if loc.flag}<span class="text-sm">{loc.flag}</span>{/if}
												<div>
													<div class="text-xs text-gray-900">{loc.country}</div>
													{#if loc.city}<div class="text-[10px] text-gray-400">{loc.city}</div>{/if}
												</div>
											</div>
										{:else}
											<span class="text-gray-300">{'\u2014'}</span>
										{/if}
									{:else if col.key === 'created_at'}
										<span class="flex items-center gap-1">
											<svg class="size-3 text-gray-400" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
												<path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
											</svg>
											{formatRelativeTime(log.created_at)}
										</span>
									{:else if col.key === 'ver_hw'}
										{getHardwareName(log.ver_hw)}
									{:else if col.key === 'flight_duration_s'}
										{formatDuration(log.flight_duration_s)}
									{:else if col.key === 'topic_count'}
										{log.topic_count}
									{:else if col.key === 'file_size'}
										{formatFileSize(log.file_size)}
									{:else}
										—
									{/if}
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</div>
</div>
