<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';
	import { darkMode } from '$lib/stores/theme';
	import NavBar from '$lib/components/shared/NavBar.svelte';
	import '../app.css';

	let { children } = $props<{ children: Snippet }>();

	// Hide the main nav sidebar on log viewer pages — it has its own topic tree sidebar
	let isLogViewer = $derived(page.url.pathname.startsWith('/log/'));

	// Initialize dark mode class on mount
	$effect(() => {
		if (browser) {
			document.documentElement.classList.toggle('dark', $darkMode);
		}
	});

	// Keyboard shortcuts
	let shortcutHelpOpen = $state(false);
	let pendingG = $state(false);
	let gTimeout: ReturnType<typeof setTimeout> | undefined;

	function handleKeydown(e: KeyboardEvent) {
		// Ignore when typing in inputs
		const tag = (e.target as HTMLElement)?.tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
		if ((e.target as HTMLElement)?.isContentEditable) return;

		const key = e.key;

		if (pendingG) {
			pendingG = false;
			if (gTimeout) clearTimeout(gTimeout);

			if (key === 'u') {
				e.preventDefault();
				goto('/');
			} else if (key === 'b') {
				e.preventDefault();
				goto('/browse');
			} else if (key === 's') {
				e.preventDefault();
				goto('/stats');
			}
			return;
		}

		if (key === '?' && !e.ctrlKey && !e.metaKey) {
			e.preventDefault();
			shortcutHelpOpen = !shortcutHelpOpen;
		} else if (key === 'g' && !e.ctrlKey && !e.metaKey) {
			pendingG = true;
			gTimeout = setTimeout(() => {
				pendingG = false;
			}, 1000);
		} else if (key === 'Escape') {
			shortcutHelpOpen = false;
		}
	}
</script>

<svelte:head>
	<link rel="preconnect" href="https://fonts.googleapis.com" />
	<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
	<link
		href="https://fonts.googleapis.com/css2?family=Inter:wght@100..900&display=swap"
		rel="stylesheet"
	/>
</svelte:head>

<svelte:window onkeydown={handleKeydown} />

{#if !isLogViewer}
	<NavBar currentPath={page.url.pathname} />
{/if}

<main class="{isLogViewer ? '' : 'lg:pl-72 overflow-x-hidden'}">
	{@render children()}
</main>

<!-- Keyboard shortcut help dialog -->
{#if shortcutHelpOpen}
	<div class="fixed inset-0 z-[100] flex items-center justify-center">
		<!-- Backdrop -->
		<!-- biome-ignore lint: backdrop click to close -->
		<div
			class="fixed inset-0 bg-gray-900/60"
			onclick={() => (shortcutHelpOpen = false)}
			role="presentation"
		></div>

		<!-- Dialog -->
		<div class="relative z-10 w-full max-w-md rounded-xl bg-white p-6 shadow-2xl dark:bg-gray-800">
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-lg font-semibold text-gray-900 dark:text-gray-100">Keyboard Shortcuts</h2>
				<button
					onclick={() => (shortcutHelpOpen = false)}
					class="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
					aria-label="Close"
				>
					<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
					</svg>
				</button>
			</div>
			<dl class="space-y-3">
				<div class="flex justify-between">
					<dt class="text-sm text-gray-700 dark:text-gray-300">Show shortcuts</dt>
					<dd><kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">?</kbd></dd>
				</div>
				<div class="flex justify-between">
					<dt class="text-sm text-gray-700 dark:text-gray-300">Go to Upload</dt>
					<dd class="flex gap-1">
						<kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">g</kbd>
						<kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">u</kbd>
					</dd>
				</div>
				<div class="flex justify-between">
					<dt class="text-sm text-gray-700 dark:text-gray-300">Go to Browse</dt>
					<dd class="flex gap-1">
						<kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">g</kbd>
						<kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">b</kbd>
					</dd>
				</div>
				<div class="flex justify-between">
					<dt class="text-sm text-gray-700 dark:text-gray-300">Go to Stats</dt>
					<dd class="flex gap-1">
						<kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">g</kbd>
						<kbd class="rounded bg-gray-100 px-2 py-0.5 text-xs font-mono text-gray-800 dark:bg-gray-700 dark:text-gray-200">s</kbd>
					</dd>
				</div>
			</dl>
		</div>
	</div>
{/if}
