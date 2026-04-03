<script lang="ts">
	import { listLogs } from '$lib/api';
	import type { LogRecord } from '$lib/types';
	import { formatDuration, formatRelativeTime } from '$lib/utils/formatters';
	import { getHardwareName } from '$lib/utils/hardwareNames';
	import LoadingSpinner from '$lib/components/shared/LoadingSpinner.svelte';

	let logs = $state<LogRecord[]>([]);
	let loading = $state(true);
	let error = $state('');

	$effect(() => {
		loadLogs();
	});

	async function loadLogs() {
		loading = true;
		error = '';
		try {
			const res = await listLogs({ page: 1, limit: 5 });
			logs = res.logs;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load recent logs';
		} finally {
			loading = false;
		}
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

<div>
	<h2 class="text-base font-semibold text-gray-900 mb-4">Recent Uploads</h2>

	{#if loading}
		<div class="flex justify-center py-8">
			<LoadingSpinner size="md" />
		</div>
	{:else if error}
		<p class="text-sm text-gray-500 py-4">{error}</p>
	{:else if logs.length === 0}
		<p class="text-sm text-gray-500 py-4">No logs uploaded yet.</p>
	{:else}
		<ul role="list" class="divide-y divide-gray-100">
			{#each logs as log (log.id)}
				{@const loc = parseLocation(log.location_name)}
				<li>
					<a
						href="/log/{log.id}"
						class="flex items-center gap-x-3 py-3 hover:bg-gray-50 rounded-md px-3 -mx-3 cursor-pointer"
					>
						<!-- Vehicle icon -->
						<img src={vehicleIconPath(log.vehicle_type)} alt={log.vehicle_type ?? ''} class="size-8 opacity-60 shrink-0" />

						<!-- Info -->
						<div class="min-w-0 flex-1">
							<div class="flex items-center gap-x-2">
								<p class="text-sm font-semibold text-gray-900">{log.vehicle_type || log.sys_name || 'Unknown'}</p>
								{#if log.ver_hw}
									<span class="inline-flex items-center rounded-md bg-gray-100 px-1.5 py-0.5 text-xs text-gray-600">{getHardwareName(log.ver_hw)}</span>
								{/if}
							</div>
							<p class="mt-0.5 text-xs text-gray-500">{log.ver_sw_release_str || 'Unknown version'} · {formatDuration(log.flight_duration_s)}</p>
						</div>

						<!-- Location -->
						<div class="shrink-0 text-right">
							{#if loc}
								<div class="flex items-center gap-1.5 justify-end">
									{#if loc.flag}<span class="text-sm">{loc.flag}</span>{/if}
									<span class="text-xs text-gray-700">{loc.city ?? loc.country}</span>
								</div>
							{/if}
							<p class="text-[10px] text-gray-400 mt-0.5 flex items-center gap-1 justify-end">
								<svg class="size-3" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
									<path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
								</svg>
								{formatRelativeTime(log.created_at)}
							</p>
						</div>
					</a>
				</li>
			{/each}
		</ul>
		<div class="mt-3 text-center">
			<a href="/browse" class="text-sm font-medium text-indigo-600 hover:text-indigo-500">Browse all flights &rarr;</a>
		</div>
	{/if}
</div>
