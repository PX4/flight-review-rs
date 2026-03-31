<script lang="ts">
	let { currentPath } = $props<{ currentPath: string }>();

	const navLinks = [
		{ name: 'Upload', href: '/' },
		{ name: 'Browse', href: '/browse' },
		{ name: 'Stats', href: '/stats' }
	];

	function isActive(href: string, current: string): boolean {
		if (href === '/') return current === '/';
		return current.startsWith(href);
	}
</script>

<div class="hidden lg:fixed lg:inset-y-0 lg:z-50 lg:flex lg:w-72 lg:flex-col">
	<div class="flex grow flex-col gap-y-5 overflow-y-auto border-r border-gray-200 bg-white px-6 pb-4">
		<div class="flex h-16 shrink-0 items-center">
			<img src="/flight-review-logo.svg" alt="Flight Review" class="h-12 w-auto" />
		</div>
		<nav class="flex flex-1 flex-col">
			<ul role="list" class="flex flex-1 flex-col gap-y-7">
				<li>
					<ul role="list" class="-mx-2 space-y-1">
						{#each navLinks as link}
							{@const active = isActive(link.href, currentPath)}
							<li>
								<a
									href={link.href}
									class="group flex gap-x-3 rounded-md p-2 text-sm font-semibold leading-6 {active
										? 'bg-gray-50 text-indigo-600'
										: 'text-gray-700 hover:bg-gray-50 hover:text-indigo-600'}"
								>
									{#if link.name === 'Upload'}
										<svg
											class="size-6 shrink-0 {active ? 'text-indigo-600' : 'text-gray-400 group-hover:text-indigo-600'}"
											fill="none"
											viewBox="0 0 24 24"
											stroke-width="1.5"
											stroke="currentColor"
											aria-hidden="true"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5m-13.5-9L12 3m0 0 4.5 4.5M12 3v13.5"
											/>
										</svg>
									{:else if link.name === 'Browse'}
										<svg
											class="size-6 shrink-0 {active ? 'text-indigo-600' : 'text-gray-400 group-hover:text-indigo-600'}"
											fill="none"
											viewBox="0 0 24 24"
											stroke-width="1.5"
											stroke="currentColor"
											aria-hidden="true"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z"
											/>
										</svg>
									{:else if link.name === 'Stats'}
										<svg
											class="size-6 shrink-0 {active ? 'text-indigo-600' : 'text-gray-400 group-hover:text-indigo-600'}"
											fill="none"
											viewBox="0 0 24 24"
											stroke-width="1.5"
											stroke="currentColor"
											aria-hidden="true"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 0 1 3 19.875v-6.75ZM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V8.625ZM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V4.125Z"
											/>
										</svg>
									{/if}
									{link.name}
								</a>
							</li>
						{/each}
					</ul>
				</li>
				<li class="mt-auto">
					<span class="text-xs font-medium text-gray-400">v2.0</span>
				</li>
			</ul>
		</nav>
	</div>
</div>
