<script lang="ts">
	let { message, onRetry } = $props<{ message: string; onRetry?: () => void }>();
</script>

<div class="rounded-md bg-red-50 p-4">
	<div class="flex">
		<div class="shrink-0">
			<svg
				class="size-5 text-red-400"
				viewBox="0 0 20 20"
				fill="currentColor"
				aria-hidden="true"
			>
				<path
					fill-rule="evenodd"
					d="M10 18a8 8 0 1 0 0-16 8 8 0 0 0 0 16ZM8.28 7.22a.75.75 0 0 0-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 1 0 1.06 1.06L10 11.06l1.72 1.72a.75.75 0 1 0 1.06-1.06L11.06 10l1.72-1.72a.75.75 0 0 0-1.06-1.06L10 8.94 8.28 7.22Z"
					clip-rule="evenodd"
				/>
			</svg>
		</div>
		<div class="ml-3">
			<p class="text-sm font-medium text-red-800">{message}</p>
		</div>
		{#if onRetry}
			<div class="ml-auto pl-3">
				<button
					type="button"
					onclick={onRetry}
					class="inline-flex rounded-md bg-red-50 px-2 py-1.5 text-sm font-medium text-red-800 hover:bg-red-100 focus:outline-none focus:ring-2 focus:ring-red-600 focus:ring-offset-2 focus:ring-offset-red-50"
				>
					Retry
				</button>
			</div>
		{/if}
	</div>
</div>
