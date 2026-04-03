<script lang="ts">
	let {
		label,
		options,
		value,
		placeholder = '',
		onChange,
	}: {
		label: string;
		options: string[];
		value: string;
		placeholder?: string;
		onChange: (value: string) => void;
	} = $props();

	let inputEl = $state<HTMLInputElement | null>(null);
	let isOpen = $state(false);
	let highlightIndex = $state(-1);
	let blurTimer: ReturnType<typeof setTimeout> | undefined;

	const listboxId = $derived(`combobox-listbox-${label.toLowerCase().replace(/\s+/g, '-')}`);

	const filtered = $derived(
		value
			? options.filter((o) => o.toLowerCase().includes(value.toLowerCase()))
			: options
	);

	function open() {
		isOpen = true;
		highlightIndex = -1;
	}

	function close() {
		isOpen = false;
		highlightIndex = -1;
	}

	function select(opt: string) {
		onChange(opt);
		close();
	}

	function handleInput(e: Event) {
		const val = (e.target as HTMLInputElement).value;
		onChange(val);
		open();
	}

	function handleFocus() {
		if (blurTimer) clearTimeout(blurTimer);
		open();
	}

	function handleBlur() {
		blurTimer = setTimeout(() => close(), 150);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!isOpen) {
			if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
				open();
				e.preventDefault();
			}
			return;
		}

		if (e.key === 'ArrowDown') {
			e.preventDefault();
			highlightIndex = highlightIndex < filtered.length - 1 ? highlightIndex + 1 : 0;
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			highlightIndex = highlightIndex > 0 ? highlightIndex - 1 : filtered.length - 1;
		} else if (e.key === 'Enter') {
			e.preventDefault();
			if (highlightIndex >= 0 && highlightIndex < filtered.length) {
				select(filtered[highlightIndex]);
			}
		} else if (e.key === 'Escape') {
			close();
		}
	}

	function toggleDropdown() {
		if (isOpen) {
			close();
		} else {
			open();
			inputEl?.focus();
		}
	}
</script>

<div>
	<!-- svelte-ignore a11y_label_has_associated_control -->
	<label class="block text-xs font-medium text-gray-500 mb-1">{label}</label>
	<div class="relative block">
		<input
			bind:this={inputEl}
			type="text"
			{placeholder}
			value={value}
			oninput={handleInput}
			onfocus={handleFocus}
			onblur={handleBlur}
			onkeydown={handleKeydown}
			class="block w-full rounded-md bg-white py-1.5 pr-10 pl-3 text-sm text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600"
			role="combobox"
			aria-expanded={isOpen}
			aria-controls={listboxId}
			aria-autocomplete="list"
			autocomplete="off"
		/>
		<button
			type="button"
			tabindex={-1}
			aria-label="Toggle {label} options"
			onclick={toggleDropdown}
			class="absolute inset-y-0 right-0 flex items-center rounded-r-md px-2"
		>
			<svg viewBox="0 0 20 20" fill="currentColor" class="size-5 text-gray-400">
				<path d="M5.22 8.22a.75.75 0 0 1 1.06 0L10 11.94l3.72-3.72a.75.75 0 1 1 1.06 1.06l-4.25 4.25a.75.75 0 0 1-1.06 0L5.22 9.28a.75.75 0 0 1 0-1.06Z" clip-rule="evenodd" fill-rule="evenodd" />
			</svg>
		</button>

		{#if isOpen}
			<div id={listboxId} role="listbox" class="absolute z-10 mt-1 max-h-60 w-full overflow-auto rounded-md bg-white py-1 text-sm shadow-lg ring-1 ring-black/5">
				{#if filtered.length === 0}
					<div class="px-3 py-2 text-gray-500">No matches</div>
				{:else}
					{#each filtered as opt, i}
						<div
							role="option"
							tabindex="-1"
							aria-selected={i === highlightIndex}
							class="cursor-pointer select-none px-3 py-2 {i === highlightIndex ? 'bg-indigo-600 text-white' : 'text-gray-900 hover:bg-indigo-600 hover:text-white'}"
							onmousedown={() => select(opt)}
							onmouseenter={() => { highlightIndex = i; }}
						>
							{opt}
						</div>
					{/each}
				{/if}
			</div>
		{/if}
	</div>
</div>
