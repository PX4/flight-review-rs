<script lang="ts">
	import type { UploadOptions, UploadResponse } from '$lib/types';
	import { uploadLog } from '$lib/api';
	import ErrorBanner from '$lib/components/shared/ErrorBanner.svelte';
	import UploadDropzone from '$lib/components/upload/UploadDropzone.svelte';
	import UploadForm from '$lib/components/upload/UploadForm.svelte';
	import UploadProgress from '$lib/components/upload/UploadProgress.svelte';
	import UploadResult from '$lib/components/upload/UploadResult.svelte';
	import RecentLogs from '$lib/components/upload/RecentLogs.svelte';

	type UploadState = 'idle' | 'selected' | 'uploading' | 'done' | 'error';

	let uploadState = $state<UploadState>('idle');
	let selectedFile = $state<File | null>(null);
	let progress = $state(0);
	let uploadStatus = $state('uploading');
	let result = $state<UploadResponse | null>(null);
	let errorMessage = $state('');
	let abortFn = $state<(() => void) | null>(null);

	function handleFileSelect(file: File) {
		selectedFile = file;
		uploadState = 'selected';
		errorMessage = '';
	}

	async function handleSubmit(opts: UploadOptions) {
		if (!selectedFile) return;

		uploadState = 'uploading';
		progress = 0;
		uploadStatus = 'uploading';
		errorMessage = '';

		const { promise, abort } = uploadLog(selectedFile, opts, (pct) => {
			progress = pct;
			if (pct >= 100) {
				uploadStatus = 'converting';
			}
		});

		abortFn = abort;

		try {
			result = await promise;
			uploadState = 'done';
		} catch (e) {
			if (e instanceof Error && e.message === 'Network error') {
				errorMessage = 'Upload cancelled or network error.';
			} else {
				errorMessage = e instanceof Error ? e.message : 'Upload failed';
			}
			uploadState = 'error';
		} finally {
			abortFn = null;
		}
	}

	function handleCancel() {
		abortFn?.();
		abortFn = null;
		uploadState = 'selected';
		progress = 0;
		uploadStatus = 'uploading';
	}

	function handleUploadAnother() {
		uploadState = 'idle';
		selectedFile = null;
		result = null;
		progress = 0;
		uploadStatus = 'uploading';
		errorMessage = '';
	}
</script>

<div class="px-4 sm:px-6 lg:px-8 max-w-3xl mx-auto py-10">
	<!-- Page header -->
	<div class="mb-8">
		<h1 class="text-2xl font-bold text-gray-900">Upload Flight Log</h1>
		<p class="mt-2 text-sm text-gray-500">Upload a .ulg file from your PX4 flight controller for analysis</p>
	</div>

	<!-- Error banner -->
	{#if uploadState === 'error' && errorMessage}
		<div class="mb-6">
			<ErrorBanner message={errorMessage} onRetry={() => { uploadState = 'selected'; errorMessage = ''; }} />
		</div>
	{/if}

	<!-- Upload state machine -->
	{#if uploadState === 'idle'}
		<UploadDropzone onFileSelect={handleFileSelect} />
	{:else if uploadState === 'selected' || uploadState === 'error'}
		{#if selectedFile}
			<UploadForm file={selectedFile} onSubmit={handleSubmit} disabled={false} />
		{/if}
	{:else if uploadState === 'uploading'}
		<UploadProgress progress={progress} status={uploadStatus} onCancel={handleCancel} />
	{:else if uploadState === 'done' && result}
		<UploadResult {result} onUploadAnother={handleUploadAnother} />
	{/if}

	<!-- Recent logs always visible -->
	<RecentLogs />
</div>
