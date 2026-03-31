<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import NavBar from '$lib/components/shared/NavBar.svelte';
	import '../app.css';

	let { children } = $props<{ children: Snippet }>();

	// Hide the main nav sidebar on log viewer pages — it has its own topic tree sidebar
	let isLogViewer = $derived(page.url.pathname.startsWith('/log/'));
</script>

<svelte:head>
	<link rel="preconnect" href="https://fonts.googleapis.com" />
	<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
	<link
		href="https://fonts.googleapis.com/css2?family=Inter:wght@100..900&display=swap"
		rel="stylesheet"
	/>
</svelte:head>

{#if !isLogViewer}
	<NavBar currentPath={page.url.pathname} />
{/if}

<main class={isLogViewer ? '' : 'lg:pl-72'}>
	{@render children()}
</main>
