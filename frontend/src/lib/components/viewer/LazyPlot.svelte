<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	let { children, height = 220 } = $props<{
		children: import('svelte').Snippet;
		height?: number;
	}>();

	let sentinel = $state<HTMLDivElement>();
	let visible = $state(false);
	let observer: IntersectionObserver | null = null;

	onMount(() => {
		if (!sentinel) return;
		observer = new IntersectionObserver(
			([entry]) => {
				if (entry.isIntersecting) {
					visible = true;
					// Once visible, stop observing — keep mounted so canvas isn't destroyed/recreated
					observer?.disconnect();
					observer = null;
				}
			},
			{ rootMargin: '200px 0px' } // start loading 200px before entering viewport
		);
		observer.observe(sentinel);
	});

	onDestroy(() => {
		observer?.disconnect();
	});
</script>

<div bind:this={sentinel}>
	{#if visible}
		{@render children()}
	{:else}
		<div
			class="rounded-lg ring-1 ring-gray-200 bg-white flex items-center justify-center"
			style="height: {height}px;"
		>
			<span class="text-xs text-gray-400">Scroll to load</span>
		</div>
	{/if}
</div>
