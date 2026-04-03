<!-- Deprecated: replaced by SearchBar, AdvancedFilters, and ActiveFilters -->
<script lang="ts">
	import type { ListFilters } from '$lib/types';

	let { filters, onChange }: {
		filters: ListFilters;
		onChange: (filters: Partial<ListFilters>) => void;
	} = $props();
</script>

<!-- This component is deprecated. Use SearchBar, AdvancedFilters, and ActiveFilters instead. -->
