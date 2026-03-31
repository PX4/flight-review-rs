import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

/**
 * FilterSidebar logic tests.
 * Tests debounce behavior and clear-all functionality
 * without rendering the Svelte component.
 */

function createDebouncer(delay: number, onChange: (filters: Record<string, string | undefined>) => void) {
	const timers: Map<string, ReturnType<typeof setTimeout>> = new Map();
	const values: Map<string, string> = new Map();

	function update(field: string, value: string) {
		values.set(field, value);
		const existing = timers.get(field);
		if (existing) clearTimeout(existing);

		const timer = setTimeout(() => {
			onChange({ [field]: value || undefined });
		}, delay);
		timers.set(field, timer);
	}

	function clearAll() {
		for (const timer of timers.values()) {
			clearTimeout(timer);
		}
		timers.clear();
		values.clear();
		onChange({ search: undefined, sys_name: undefined, ver_hw: undefined });
	}

	function dispose() {
		for (const timer of timers.values()) {
			clearTimeout(timer);
		}
		timers.clear();
	}

	return { update, clearAll, dispose, values };
}

describe('FilterSidebar', () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	describe('debounce behavior', () => {
		it('does not call onChange immediately on input', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', 'test');
			expect(onChange).not.toHaveBeenCalled();

			debouncer.dispose();
		});

		it('calls onChange after 300ms debounce', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', 'test');
			vi.advanceTimersByTime(300);

			expect(onChange).toHaveBeenCalledOnce();
			expect(onChange).toHaveBeenCalledWith({ search: 'test' });

			debouncer.dispose();
		});

		it('resets debounce timer on subsequent input', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', 'te');
			vi.advanceTimersByTime(200);
			expect(onChange).not.toHaveBeenCalled();

			debouncer.update('search', 'test');
			vi.advanceTimersByTime(200);
			expect(onChange).not.toHaveBeenCalled();

			vi.advanceTimersByTime(100);
			expect(onChange).toHaveBeenCalledOnce();
			expect(onChange).toHaveBeenCalledWith({ search: 'test' });

			debouncer.dispose();
		});

		it('debounces each field independently', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', 'hello');
			vi.advanceTimersByTime(150);

			debouncer.update('sys_name', 'PX4');
			vi.advanceTimersByTime(150);

			// search should have fired
			expect(onChange).toHaveBeenCalledWith({ search: 'hello' });

			vi.advanceTimersByTime(150);
			// sys_name should have fired
			expect(onChange).toHaveBeenCalledWith({ sys_name: 'PX4' });
			expect(onChange).toHaveBeenCalledTimes(2);

			debouncer.dispose();
		});

		it('sends undefined for empty string values', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', '');
			vi.advanceTimersByTime(300);

			expect(onChange).toHaveBeenCalledWith({ search: undefined });

			debouncer.dispose();
		});
	});

	describe('clear all filters', () => {
		it('calls onChange with all fields set to undefined', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.clearAll();

			expect(onChange).toHaveBeenCalledOnce();
			expect(onChange).toHaveBeenCalledWith({
				search: undefined,
				sys_name: undefined,
				ver_hw: undefined,
			});

			debouncer.dispose();
		});

		it('cancels pending debounced calls', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', 'test');
			debouncer.update('sys_name', 'PX4');

			debouncer.clearAll();

			// Only the clearAll call should have been made
			expect(onChange).toHaveBeenCalledOnce();
			expect(onChange).toHaveBeenCalledWith({
				search: undefined,
				sys_name: undefined,
				ver_hw: undefined,
			});

			// Advance timers past debounce - no additional calls should happen
			vi.advanceTimersByTime(500);
			expect(onChange).toHaveBeenCalledOnce();

			debouncer.dispose();
		});

		it('resets local values', () => {
			const onChange = vi.fn();
			const debouncer = createDebouncer(300, onChange);

			debouncer.update('search', 'test');
			debouncer.update('sys_name', 'PX4');
			debouncer.update('ver_hw', 'Pixhawk');

			debouncer.clearAll();

			expect(debouncer.values.size).toBe(0);

			debouncer.dispose();
		});
	});
});
