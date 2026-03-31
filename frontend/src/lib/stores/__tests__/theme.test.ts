import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

// Mock $app/environment before importing the module
vi.mock('$app/environment', () => ({ browser: true }));

describe('darkMode store', () => {
	beforeEach(() => {
		// Reset DOM and localStorage
		document.documentElement.classList.remove('dark');
		localStorage.clear();
		// Re-import for fresh store each test
		vi.resetModules();
	});

	it('defaults to light when no preference is stored and prefers-color-scheme is light', async () => {
		window.matchMedia = vi.fn().mockReturnValue({ matches: false });
		const { darkMode } = await import('../theme');
		expect(get(darkMode)).toBe(false);
	});

	it('defaults to dark when prefers-color-scheme is dark and nothing stored', async () => {
		window.matchMedia = vi.fn().mockReturnValue({ matches: true });
		const { darkMode } = await import('../theme');
		expect(get(darkMode)).toBe(true);
	});

	it('reads stored preference over system preference', async () => {
		localStorage.setItem('theme', 'dark');
		window.matchMedia = vi.fn().mockReturnValue({ matches: false });
		const { darkMode } = await import('../theme');
		expect(get(darkMode)).toBe(true);
	});

	it('toggle() switches from light to dark', async () => {
		window.matchMedia = vi.fn().mockReturnValue({ matches: false });
		const { darkMode } = await import('../theme');
		expect(get(darkMode)).toBe(false);

		darkMode.toggle();

		expect(get(darkMode)).toBe(true);
		expect(localStorage.getItem('theme')).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);
	});

	it('toggle() switches from dark to light', async () => {
		localStorage.setItem('theme', 'dark');
		window.matchMedia = vi.fn().mockReturnValue({ matches: false });
		const { darkMode } = await import('../theme');
		expect(get(darkMode)).toBe(true);

		darkMode.toggle();

		expect(get(darkMode)).toBe(false);
		expect(localStorage.getItem('theme')).toBe('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});

	it('set() persists and updates classList', async () => {
		window.matchMedia = vi.fn().mockReturnValue({ matches: false });
		const { darkMode } = await import('../theme');

		darkMode.set(true);
		expect(get(darkMode)).toBe(true);
		expect(localStorage.getItem('theme')).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);

		darkMode.set(false);
		expect(get(darkMode)).toBe(false);
		expect(localStorage.getItem('theme')).toBe('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});
});
