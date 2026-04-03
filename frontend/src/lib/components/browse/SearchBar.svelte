<script lang="ts">
	import type { ListFilters } from '$lib/types';

	let { value = '', onChange }: {
		value: string;
		onChange: (filters: Partial<ListFilters>) => void;
	} = $props();

	let inputValue = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => {
		inputValue = value;
	});

	function handleInput(e: Event) {
		inputValue = (e.target as HTMLInputElement).value;
		if (timer) clearTimeout(timer);
		timer = setTimeout(() => {
			onChange({ search: inputValue || undefined });
		}, 300);
	}

	function handleClear() {
		inputValue = '';
		if (timer) clearTimeout(timer);
		onChange({ search: undefined });
	}
</script>

<div class="relative">
	<div class="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
		<svg class="size-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
			<path stroke-linecap="round" stroke-linejoin="round" d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z" />
		</svg>
	</div>
	<input
		type="text"
		placeholder="Search logs..."
		value={inputValue}
		oninput={handleInput}
		class="block w-full rounded-md bg-white py-2 pl-10 pr-10 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none"
	/>
	{#if inputValue}
		<button
			type="button"
			aria-label="Clear search"
			onclick={handleClear}
			class="absolute inset-y-0 right-0 flex items-center pr-3 text-gray-400 hover:text-gray-600"
		>
			<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
			</svg>
		</button>
	{/if}
</div>
