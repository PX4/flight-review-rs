<script lang="ts">
	let { size = 'md' } = $props<{ size?: 'sm' | 'md' | 'lg' }>();

	const sizeClass = $derived(
		size === 'sm' ? 'size-4' : size === 'lg' ? 'size-8' : 'size-6'
	);
</script>

<svg
	class="animate-spin {sizeClass} text-indigo-600"
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
