<script lang="ts">
	let { total, page, pageSize, onChange }: {
		total: number;
		page: number;
		pageSize: number;
		onChange: (page: number, pageSize: number) => void;
	} = $props();

	let totalPages = $derived(Math.max(1, Math.ceil(total / pageSize)));

	let showingFrom = $derived(total === 0 ? 0 : (page - 1) * pageSize + 1);
	let showingTo = $derived(Math.min(page * pageSize, total));

	let visiblePages = $derived.by(() => {
		const pages: (number | 'ellipsis')[] = [];
		if (totalPages <= 5) {
			for (let i = 1; i <= totalPages; i++) pages.push(i);
			return pages;
		}

		// Always show first page
		pages.push(1);

		if (page > 3) {
			pages.push('ellipsis');
		}

		// Pages around current
		const start = Math.max(2, page - 1);
		const end = Math.min(totalPages - 1, page + 1);
		for (let i = start; i <= end; i++) {
			pages.push(i);
		}

		if (page < totalPages - 2) {
			pages.push('ellipsis');
		}

		// Always show last page
		pages.push(totalPages);

		return pages;
	});

	function goTo(p: number) {
		if (p >= 1 && p <= totalPages && p !== page) {
			onChange(p, pageSize);
		}
	}
</script>

<nav class="flex items-center justify-between border-t border-gray-200 px-4 py-3 sm:px-0 mt-4">
	<div class="hidden sm:block">
		<p class="text-sm text-gray-500">
			Showing <span class="font-medium text-gray-900">{showingFrom.toLocaleString()}</span> to
			<span class="font-medium text-gray-900">{showingTo.toLocaleString()}</span> of
			<span class="font-medium text-gray-900">{total.toLocaleString()}</span> results
		</p>
	</div>
	<div class="flex flex-1 justify-between sm:justify-end gap-x-1">
		<button
			type="button"
			class="relative inline-flex items-center rounded-md bg-white px-3 py-2 text-sm font-semibold text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed"
			disabled={page <= 1}
			onclick={() => goTo(page - 1)}
		>
			Previous
		</button>

		{#each visiblePages as p}
			{#if p === 'ellipsis'}
				<span class="relative inline-flex items-center px-3 py-2 text-sm text-gray-500">...</span>
			{:else}
				<button
					type="button"
					class="relative inline-flex items-center rounded-md px-3 py-2 text-sm font-semibold {p === page
						? 'bg-indigo-500 text-white'
						: 'bg-white text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50'}"
					onclick={() => goTo(p)}
				>
					{p}
				</button>
			{/if}
		{/each}

		<button
			type="button"
			class="relative inline-flex items-center rounded-md bg-white px-3 py-2 text-sm font-semibold text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed"
			disabled={page >= totalPages}
			onclick={() => goTo(page + 1)}
		>
			Next
		</button>
	</div>
</nav>
