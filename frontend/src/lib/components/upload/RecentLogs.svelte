<script lang="ts">
	import { listLogs } from '$lib/api';
	import type { LogRecord } from '$lib/types';
	import { formatDuration, formatFileSize, formatRelativeTime } from '$lib/utils/formatters';
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
				<li>
					<a
						href="/log/{log.id}"
						class="flex items-center justify-between gap-x-6 py-4 hover:bg-gray-50 rounded-md px-3 -mx-3 cursor-pointer"
					>
						<div class="min-w-0">
							<div class="flex items-center gap-x-3">
								<p class="text-sm font-semibold text-gray-900">{log.vehicle_name || log.sys_name || 'Unknown'}</p>
								{#if log.ver_hw}
									<span class="inline-flex items-center rounded-md bg-gray-100 px-2 py-1 text-xs font-medium text-gray-600 ring-1 ring-gray-200">{log.ver_hw}</span>
								{/if}
							</div>
							<p class="mt-1 text-xs text-gray-500">{log.topic_count} topics · {formatFileSize(log.file_size)}</p>
						</div>
						<div class="flex items-center gap-x-4 text-right shrink-0">
							<p class="text-sm text-gray-500">{formatDuration(log.flight_duration_s)}</p>
							<p class="text-xs text-gray-500">{formatRelativeTime(log.created_at)}</p>
						</div>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</div>
