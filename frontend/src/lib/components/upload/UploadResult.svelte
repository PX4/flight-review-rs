<script lang="ts">
	import type { UploadResponse } from '$lib/types';
	import { formatDuration } from '$lib/utils/formatters';

	let { result, onUploadAnother } = $props<{
		result: UploadResponse;
		onUploadAnother: () => void;
	}>();

	let showToken = $state(false);
	let copied = $state(false);

	async function copyToken() {
		await navigator.clipboard.writeText(result.delete_token);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 shadow-sm p-6 mb-8">
	<!-- Success header -->
	<div class="flex items-center gap-3 mb-6">
		<div class="flex items-center justify-center size-10 rounded-full bg-emerald-100">
			<svg class="size-6 text-emerald-600" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5" />
			</svg>
		</div>
		<div>
			<h3 class="text-base font-semibold text-gray-900">Upload Successful</h3>
			<p class="text-sm text-gray-500">Your log has been processed and is ready for review</p>
		</div>
	</div>

	<!-- Log details -->
	<dl class="grid grid-cols-2 gap-x-4 gap-y-3 text-sm mb-6">
		<div>
			<dt class="text-gray-500">Log ID</dt>
			<dd class="font-medium text-gray-900 font-mono text-xs mt-0.5">{result.id}</dd>
		</div>
		<div>
			<dt class="text-gray-500">Filename</dt>
			<dd class="font-medium text-gray-900 mt-0.5">{result.filename}</dd>
		</div>
		{#if result.sys_name}
			<div>
				<dt class="text-gray-500">Vehicle</dt>
				<dd class="font-medium text-gray-900 mt-0.5">{result.sys_name}</dd>
			</div>
		{/if}
		{#if result.ver_hw}
			<div>
				<dt class="text-gray-500">Hardware</dt>
				<dd class="font-medium text-gray-900 mt-0.5">{result.ver_hw}</dd>
			</div>
		{/if}
		<div>
			<dt class="text-gray-500">Flight Duration</dt>
			<dd class="font-medium text-gray-900 mt-0.5">{formatDuration(result.flight_duration_s)}</dd>
		</div>
		<div>
			<dt class="text-gray-500">Topics</dt>
			<dd class="font-medium text-gray-900 mt-0.5">{result.topic_count}</dd>
		</div>
		<div>
			<dt class="text-gray-500">Visibility</dt>
			<dd class="mt-0.5">
				{#if result.is_public}
					<span class="inline-flex items-center rounded-md bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700 ring-1 ring-emerald-200">Public</span>
				{:else}
					<span class="inline-flex items-center rounded-md bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-600 ring-1 ring-gray-200">Private</span>
				{/if}
			</dd>
		</div>
	</dl>

	<!-- Delete token -->
	<div class="rounded-md bg-amber-50 border border-amber-200 px-4 py-3 mb-6">
		<div class="flex items-start gap-3">
			<svg class="size-5 text-amber-600 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z" />
			</svg>
			<div class="flex-1 min-w-0">
				<p class="text-sm font-medium text-amber-800">Delete Token</p>
				<p class="text-xs text-amber-700 mt-0.5">Save this token to delete the log later. It cannot be recovered.</p>
				{#if showToken}
					<div class="flex items-center gap-2 mt-2">
						<code class="text-xs bg-amber-100 px-2 py-1 rounded font-mono break-all">{result.delete_token}</code>
						<button
							type="button"
							onclick={copyToken}
							class="shrink-0 rounded bg-amber-200 px-2 py-1 text-xs font-medium text-amber-800 hover:bg-amber-300"
						>
							{copied ? 'Copied!' : 'Copy'}
						</button>
					</div>
				{:else}
					<button
						type="button"
						onclick={() => (showToken = true)}
						class="mt-2 text-xs font-medium text-amber-800 underline hover:text-amber-900"
					>
						Reveal token
					</button>
				{/if}
			</div>
		</div>
	</div>

	<!-- Actions -->
	<div class="flex items-center gap-3">
		<a
			href="/log/{result.id}"
			class="rounded-md bg-indigo-500 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-400 focus:outline-2 focus:outline-offset-2 focus:outline-indigo-500"
		>
			View Log
		</a>
		<button
			type="button"
			onclick={onUploadAnother}
			class="rounded-md bg-white px-4 py-2 text-sm font-semibold text-gray-700 ring-1 ring-gray-300 hover:bg-gray-50"
		>
			Upload Another
		</button>
	</div>
</div>
