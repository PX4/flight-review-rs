<script lang="ts">
	let { progress, status, onCancel } = $props<{
		progress: number;
		status: string;
		onCancel: () => void;
	}>();

	const isConverting = $derived(status === 'converting');
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 shadow-sm p-6 mb-8">
	<div class="text-center">
		{#if isConverting}
			<!-- Indeterminate spinner for conversion phase -->
			<div class="flex justify-center mb-4">
				<svg
					class="animate-spin size-8 text-indigo-500"
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
					aria-hidden="true"
				>
					<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
					<path
						class="opacity-75"
						fill="currentColor"
						d="M4 12a8 8 0 0 1 8-8V0C5.373 0 0 5.373 0 12h4Zm2 5.291A7.962 7.962 0 0 1 4 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647Z"
					/>
				</svg>
			</div>
			<p class="text-sm font-medium text-gray-700">Converting...</p>
			<p class="text-xs text-gray-500 mt-1">Parsing ULog and generating Parquet files</p>
		{:else}
			<!-- Determinate progress bar for upload phase -->
			<p class="text-sm font-medium text-gray-700 mb-3">Uploading... {Math.round(progress)}%</p>
			<div class="w-full bg-gray-200 rounded-full h-2.5">
				<div
					class="bg-indigo-500 h-2.5 rounded-full transition-all duration-300"
					style="width: {progress}%"
				></div>
			</div>
		{/if}

		<button
			type="button"
			onclick={onCancel}
			class="mt-4 rounded-md bg-white px-3 py-1.5 text-sm font-medium text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50"
		>
			Cancel
		</button>
	</div>
</div>
