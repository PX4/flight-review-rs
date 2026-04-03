<script lang="ts">
	let { onFileSelect } = $props<{ onFileSelect: (file: File) => void }>();

	let isDragOver = $state(false);
	let error = $state('');
	let fileInput: HTMLInputElement;

	function validateAndSelect(file: File) {
		if (!file.name.toLowerCase().endsWith('.ulg')) {
			error = 'Invalid file type. Please select a .ulg file.';
			return;
		}
		error = '';
		onFileSelect(file);
	}

	function handleDrop(e: DragEvent) {
		isDragOver = false;
		const file = e.dataTransfer?.files[0];
		if (file) validateAndSelect(file);
	}

	function handleFileInput(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (file) validateAndSelect(file);
	}
</script>

<div class="mb-8">
	<button
		type="button"
		class="flex w-full justify-center rounded-lg border-2 border-dashed px-6 py-8 sm:py-16 transition-colors cursor-pointer {isDragOver
			? 'border-indigo-500 bg-indigo-50'
			: 'border-gray-300 hover:border-indigo-500/50'}"
		ondragover={(e) => {
			e.preventDefault();
			isDragOver = true;
		}}
		ondragleave={() => (isDragOver = false)}
		ondrop={(e) => {
			e.preventDefault();
			handleDrop(e);
		}}
		onclick={() => fileInput.click()}
	>
		<div class="text-center">
			<svg
				class="mx-auto size-8 sm:size-12 text-gray-500"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1"
				stroke="currentColor"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
				/>
			</svg>
			<div class="mt-4 flex text-sm/6 text-gray-600">
				<span class="font-semibold text-indigo-400 hover:text-indigo-300">Click to upload</span>
				<span class="pl-1">or drag and drop</span>
			</div>
			<p class="text-xs/5 text-gray-500 mt-1">ULog files up to 500 MB</p>
		</div>
	</button>
	<input
		bind:this={fileInput}
		type="file"
		accept=".ulg"
		class="hidden"
		onchange={handleFileInput}
	/>
	{#if error}
		<p class="mt-2 text-sm text-red-600">{error}</p>
	{/if}
</div>
