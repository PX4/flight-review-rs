<script lang="ts">
	import { darkMode } from '$lib/stores/theme';

	let { currentPath } = $props<{ currentPath: string }>();

	let mobileMenuOpen = $state(false);

	const navLinks = [
		{ name: 'Upload', href: '/' },
		{ name: 'Browse', href: '/browse' },
		{ name: 'Stats', href: '/stats' }
	];

	function isActive(href: string, current: string): boolean {
		if (href === '/') return current === '/';
		return current.startsWith(href);
	}

	function closeMobileMenu() {
		mobileMenuOpen = false;
	}
</script>

<!-- Desktop sidebar -->
<div class="hidden lg:fixed lg:inset-y-0 lg:z-50 lg:flex lg:w-72 lg:flex-col">
	<div class="flex grow flex-col gap-y-5 overflow-y-auto border-r border-gray-200 bg-white px-6 pb-4 dark:border-gray-700 dark:bg-gray-900">
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
										? 'bg-gray-50 text-indigo-600 dark:bg-gray-800 dark:text-indigo-400'
										: 'text-gray-700 hover:bg-gray-50 hover:text-indigo-600 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-indigo-400'}"
								>
									{#if link.name === 'Upload'}
										<svg
											class="size-6 shrink-0 {active ? 'text-indigo-600 dark:text-indigo-400' : 'text-gray-400 group-hover:text-indigo-600 dark:text-gray-500 dark:group-hover:text-indigo-400'}"
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
											class="size-6 shrink-0 {active ? 'text-indigo-600 dark:text-indigo-400' : 'text-gray-400 group-hover:text-indigo-600 dark:text-gray-500 dark:group-hover:text-indigo-400'}"
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
											class="size-6 shrink-0 {active ? 'text-indigo-600 dark:text-indigo-400' : 'text-gray-400 group-hover:text-indigo-600 dark:text-gray-500 dark:group-hover:text-indigo-400'}"
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
				<li class="mt-auto space-y-3">
					<!-- Dark mode toggle -->
					<button
						onclick={() => darkMode.toggle()}
						class="flex w-full items-center gap-x-3 rounded-md p-2 text-sm font-semibold leading-6 text-gray-700 hover:bg-gray-50 hover:text-indigo-600 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-indigo-400"
						aria-label="Toggle dark mode"
					>
						{#if $darkMode}
							<!-- Sun icon -->
							<svg class="size-6 shrink-0 text-gray-400 dark:text-gray-500" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
								<path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-4.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z" />
							</svg>
							Light mode
						{:else}
							<!-- Moon icon -->
							<svg class="size-6 shrink-0 text-gray-400" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
								<path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z" />
							</svg>
							Dark mode
						{/if}
					</button>
					<span class="block text-xs font-medium text-gray-400 dark:text-gray-500">v2.0</span>
				</li>
			</ul>
		</nav>
	</div>
</div>

<!-- Mobile top bar -->
<div class="sticky top-0 z-40 flex items-center gap-x-4 border-b border-gray-200 bg-white px-4 py-3 shadow-sm lg:hidden dark:border-gray-700 dark:bg-gray-900">
	<button
		type="button"
		class="-m-2.5 p-2.5 text-gray-700 dark:text-gray-300"
		aria-label="Open menu"
		onclick={() => (mobileMenuOpen = true)}
	>
		<svg class="size-6" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
			<path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" />
		</svg>
	</button>
	<div class="flex-1">
		<img src="/flight-review-logo.svg" alt="Flight Review" class="h-8 w-auto" />
	</div>
	<button
		onclick={() => darkMode.toggle()}
		class="p-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
		aria-label="Toggle dark mode"
	>
		{#if $darkMode}
			<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-4.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z" />
			</svg>
		{:else}
			<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
				<path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z" />
			</svg>
		{/if}
	</button>
</div>

<!-- Mobile slide-over menu -->
{#if mobileMenuOpen}
	<!-- Backdrop -->
	<div class="fixed inset-0 z-50 lg:hidden">
		<!-- biome-ignore lint: backdrop click to close -->
		<div
			class="fixed inset-0 bg-gray-900/80"
			onclick={closeMobileMenu}
			role="presentation"
		></div>

		<!-- Panel -->
		<div class="fixed inset-y-0 left-0 z-50 w-full max-w-xs overflow-y-auto bg-white px-6 pb-4 dark:bg-gray-900">
			<div class="flex h-16 items-center justify-between">
				<img src="/flight-review-logo.svg" alt="Flight Review" class="h-10 w-auto" />
				<button
					type="button"
					class="-m-2.5 p-2.5 text-gray-700 dark:text-gray-300"
					aria-label="Close menu"
					onclick={closeMobileMenu}
				>
					<svg class="size-6" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
						<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
					</svg>
				</button>
			</div>
			<nav class="mt-4">
				<ul role="list" class="-mx-2 space-y-1">
					{#each navLinks as link}
						{@const active = isActive(link.href, currentPath)}
						<li>
							<a
								href={link.href}
								onclick={closeMobileMenu}
								class="group flex gap-x-3 rounded-md p-2 text-sm font-semibold leading-6 {active
									? 'bg-gray-50 text-indigo-600 dark:bg-gray-800 dark:text-indigo-400'
									: 'text-gray-700 hover:bg-gray-50 hover:text-indigo-600 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-indigo-400'}"
							>
								{#if link.name === 'Upload'}
									<svg class="size-6 shrink-0 {active ? 'text-indigo-600 dark:text-indigo-400' : 'text-gray-400 group-hover:text-indigo-600'}" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
										<path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5m-13.5-9L12 3m0 0 4.5 4.5M12 3v13.5" />
									</svg>
								{:else if link.name === 'Browse'}
									<svg class="size-6 shrink-0 {active ? 'text-indigo-600 dark:text-indigo-400' : 'text-gray-400 group-hover:text-indigo-600'}" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
										<path stroke-linecap="round" stroke-linejoin="round" d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
									</svg>
								{:else if link.name === 'Stats'}
									<svg class="size-6 shrink-0 {active ? 'text-indigo-600 dark:text-indigo-400' : 'text-gray-400 group-hover:text-indigo-600'}" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true">
										<path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 0 1 3 19.875v-6.75ZM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V8.625ZM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V4.125Z" />
									</svg>
								{/if}
								{link.name}
							</a>
						</li>
					{/each}
				</ul>
			</nav>
			<div class="mt-8">
				<span class="text-xs font-medium text-gray-400 dark:text-gray-500">v2.0</span>
			</div>
		</div>
	</div>
{/if}
