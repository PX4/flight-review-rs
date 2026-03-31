<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { listLogs } from '$lib/api';
	import type { ListFilters, LogRecord, ListResponse } from '$lib/types';
	import FilterSidebar from '$lib/components/browse/FilterSidebar.svelte';
	import LogTable from '$lib/components/browse/LogTable.svelte';
	import PaginationControls from '$lib/components/browse/PaginationControls.svelte';

	const DEFAULT_PAGE_SIZE = 20;

	// Read filters from URL search params
	let filters = $derived.by((): ListFilters => {
		const params = page.url.searchParams;
		return {
			search: params.get('search') || undefined,
			sys_name: params.get('sys_name') || undefined,
			ver_hw: params.get('ver_hw') || undefined,
			page: parseInt(params.get('page') || '1', 10),
			limit: parseInt(params.get('limit') || String(DEFAULT_PAGE_SIZE), 10),
		};
	});

	let logs = $state<LogRecord[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let sortField = $state<string | null>('created_at');
	let sortDir = $state<'asc' | 'desc'>('desc');
	let mobileFiltersOpen = $state(false);

	function updateUrl(partial: Partial<ListFilters>) {
		const params = new URLSearchParams(page.url.searchParams);

		for (const [key, value] of Object.entries(partial)) {
			if (value === undefined || value === '') {
				params.delete(key);
			} else {
				params.set(key, String(value));
			}
		}

		// Reset to page 1 when filters change (but not when page itself changes)
		if (!('page' in partial)) {
			params.delete('page');
		}

		const search = params.toString();
		goto(`/browse${search ? `?${search}` : ''}`, { replaceState: true, keepFocus: true });
	}

	function handleFilterChange(partial: Partial<ListFilters>) {
		updateUrl(partial);
	}

	function handlePageChange(newPage: number, pageSize: number) {
		updateUrl({ page: newPage, limit: pageSize });
	}

	function handleSort(field: string) {
		if (sortField === field) {
			sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		} else {
			sortField = field;
			sortDir = 'desc';
		}
	}

	// Fetch data when filters change
	$effect(() => {
		const currentFilters = filters;
		loading = true;
		error = null;

		listLogs(currentFilters)
			.then((res: ListResponse) => {
				logs = res.logs;
				total = res.total;
				loading = false;
			})
			.catch((err: Error) => {
				error = err.message || 'Failed to load logs';
				loading = false;
			});
	});
</script>

<svelte:head>
	<title>Browse Flight Logs - Flight Review</title>
</svelte:head>

<div class="px-4 sm:px-6 lg:px-8 py-8">
	<!-- Header -->
	<div class="sm:flex sm:items-center mb-6">
		<div class="sm:flex-auto">
			<h1 class="text-base font-semibold text-gray-900 dark:text-gray-100">Flight Logs</h1>
			{#if !loading && !error}
				<p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{total.toLocaleString()} logs found</p>
			{/if}
		</div>
	</div>

	<!-- Mobile filter toggle -->
	<div class="lg:hidden mb-4">
		<button
			onclick={() => (mobileFiltersOpen = !mobileFiltersOpen)}
			class="flex items-center gap-2 rounded-md bg-white px-3 py-2 text-sm font-medium text-gray-700 shadow-sm ring-1 ring-gray-300 hover:bg-gray-50 dark:bg-gray-800 dark:text-gray-300 dark:ring-gray-600 dark:hover:bg-gray-700"
		>
			<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M12 3c2.755 0 5.455.232 8.083.678.533.09.917.556.917 1.096v1.044a2.25 2.25 0 0 1-.659 1.591l-5.432 5.432a2.25 2.25 0 0 0-.659 1.591v2.927a2.25 2.25 0 0 1-1.244 2.013L9.75 21v-6.568a2.25 2.25 0 0 0-.659-1.591L3.659 7.409A2.25 2.25 0 0 1 3 5.818V4.774c0-.54.384-1.006.917-1.096A48.32 48.32 0 0 1 12 3Z" />
			</svg>
			{mobileFiltersOpen ? 'Hide Filters' : 'Show Filters'}
		</button>
	</div>

	<!-- Mobile collapsible filters -->
	{#if mobileFiltersOpen}
		<div class="lg:hidden mb-6">
			<FilterSidebar {filters} onChange={handleFilterChange} />
		</div>
	{/if}

	<div class="flex gap-8">
		<!-- Filter sidebar on desktop -->
		<div class="hidden lg:block w-56 shrink-0">
			<FilterSidebar {filters} onChange={handleFilterChange} />
		</div>

		<!-- Main content area -->
		<div class="min-w-0 flex-1">
			{#if loading}
				<!-- Loading skeleton -->
				<div class="animate-pulse space-y-4">
					<div class="h-8 bg-gray-200 rounded w-full dark:bg-gray-700"></div>
					{#each Array(8) as _}
						<div class="h-12 bg-gray-100 rounded w-full dark:bg-gray-800"></div>
					{/each}
				</div>
			{:else if error}
				<!-- Error state -->
				<div class="rounded-md bg-red-50 p-4 dark:bg-red-900/20">
					<div class="flex">
						<div class="shrink-0">
							<svg class="size-5 text-red-400" viewBox="0 0 20 20" fill="currentColor">
								<path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clip-rule="evenodd" />
							</svg>
						</div>
						<div class="ml-3">
							<h3 class="text-sm font-medium text-red-800 dark:text-red-300">Error loading logs</h3>
							<p class="mt-1 text-sm text-red-700 dark:text-red-400">{error}</p>
						</div>
					</div>
				</div>
			{:else if logs.length === 0}
				<!-- Empty state -->
				<div class="text-center py-12">
					<svg class="mx-auto size-12 text-gray-400" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" />
					</svg>
					<h3 class="mt-2 text-sm font-semibold text-gray-900 dark:text-gray-100">No logs found</h3>
					<p class="mt-1 text-sm text-gray-500 dark:text-gray-400">Try adjusting your filters or upload a new log.</p>
				</div>
			{:else}
				<!-- Horizontally scrollable table wrapper for mobile -->
				<div class="overflow-x-auto">
					<LogTable {logs} {sortField} {sortDir} onSort={handleSort} />
				</div>

				<PaginationControls
					{total}
					page={filters.page}
					pageSize={filters.limit}
					onChange={handlePageChange}
				/>
			{/if}
		</div>
	</div>
</div>
