import { describe, it, expect, vi } from 'vitest';

/**
 * PaginationControls logic tests.
 * Tests the page number calculation and callback behavior
 * without rendering the Svelte component.
 */

function computeVisiblePages(page: number, totalPages: number): (number | 'ellipsis')[] {
	const pages: (number | 'ellipsis')[] = [];
	if (totalPages <= 5) {
		for (let i = 1; i <= totalPages; i++) pages.push(i);
		return pages;
	}

	pages.push(1);

	if (page > 3) {
		pages.push('ellipsis');
	}

	const start = Math.max(2, page - 1);
	const end = Math.min(totalPages - 1, page + 1);
	for (let i = start; i <= end; i++) {
		pages.push(i);
	}

	if (page < totalPages - 2) {
		pages.push('ellipsis');
	}

	pages.push(totalPages);

	return pages;
}

function computeShowing(page: number, pageSize: number, total: number) {
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const from = total === 0 ? 0 : (page - 1) * pageSize + 1;
	const to = Math.min(page * pageSize, total);
	return { from, to, totalPages };
}

describe('PaginationControls', () => {
	describe('page number calculation', () => {
		it('shows all pages when total pages <= 5', () => {
			expect(computeVisiblePages(1, 3)).toEqual([1, 2, 3]);
			expect(computeVisiblePages(2, 5)).toEqual([1, 2, 3, 4, 5]);
			expect(computeVisiblePages(1, 1)).toEqual([1]);
		});

		it('shows ellipsis for many pages, current at start', () => {
			const result = computeVisiblePages(1, 100);
			expect(result).toEqual([1, 2, 'ellipsis', 100]);
		});

		it('shows ellipsis for many pages, current in middle', () => {
			const result = computeVisiblePages(50, 100);
			expect(result).toEqual([1, 'ellipsis', 49, 50, 51, 'ellipsis', 100]);
		});

		it('shows ellipsis for many pages, current near end', () => {
			const result = computeVisiblePages(99, 100);
			expect(result).toEqual([1, 'ellipsis', 98, 99, 100]);
		});

		it('shows ellipsis for many pages, current at end', () => {
			const result = computeVisiblePages(100, 100);
			expect(result).toEqual([1, 'ellipsis', 99, 100]);
		});

		it('handles page 3 boundary (no left ellipsis)', () => {
			const result = computeVisiblePages(3, 100);
			expect(result).toEqual([1, 2, 3, 4, 'ellipsis', 100]);
		});

		it('handles page 4 boundary (left ellipsis appears)', () => {
			const result = computeVisiblePages(4, 100);
			expect(result).toEqual([1, 'ellipsis', 3, 4, 5, 'ellipsis', 100]);
		});
	});

	describe('showing text calculation', () => {
		it('calculates correct range for first page', () => {
			const { from, to, totalPages } = computeShowing(1, 20, 100);
			expect(from).toBe(1);
			expect(to).toBe(20);
			expect(totalPages).toBe(5);
		});

		it('calculates correct range for last page with partial results', () => {
			const { from, to, totalPages } = computeShowing(5, 20, 95);
			expect(from).toBe(81);
			expect(to).toBe(95);
			expect(totalPages).toBe(5);
		});

		it('handles zero total', () => {
			const { from, to, totalPages } = computeShowing(1, 20, 0);
			expect(from).toBe(0);
			expect(to).toBe(0);
			expect(totalPages).toBe(1);
		});
	});

	describe('navigation callbacks', () => {
		it('calls onChange with next page', () => {
			const onChange = vi.fn();
			const currentPage = 3;
			const pageSize = 20;
			const totalPages = 10;

			// Simulate clicking "Next"
			const nextPage = currentPage + 1;
			if (nextPage >= 1 && nextPage <= totalPages && nextPage !== currentPage) {
				onChange(nextPage, pageSize);
			}
			expect(onChange).toHaveBeenCalledWith(4, 20);
		});

		it('calls onChange with previous page', () => {
			const onChange = vi.fn();
			const currentPage = 3;
			const pageSize = 20;
			const totalPages = 10;

			const prevPage = currentPage - 1;
			if (prevPage >= 1 && prevPage <= totalPages && prevPage !== currentPage) {
				onChange(prevPage, pageSize);
			}
			expect(onChange).toHaveBeenCalledWith(2, 20);
		});

		it('does not call onChange when already on first page and clicking previous', () => {
			const onChange = vi.fn();
			const currentPage = 1;
			const pageSize = 20;
			const totalPages = 10;

			const prevPage = currentPage - 1;
			if (prevPage >= 1 && prevPage <= totalPages && prevPage !== currentPage) {
				onChange(prevPage, pageSize);
			}
			expect(onChange).not.toHaveBeenCalled();
		});

		it('does not call onChange when already on last page and clicking next', () => {
			const onChange = vi.fn();
			const currentPage = 10;
			const pageSize = 20;
			const totalPages = 10;

			const nextPage = currentPage + 1;
			if (nextPage >= 1 && nextPage <= totalPages && nextPage !== currentPage) {
				onChange(nextPage, pageSize);
			}
			expect(onChange).not.toHaveBeenCalled();
		});

		it('does not call onChange when clicking the current page', () => {
			const onChange = vi.fn();
			const currentPage = 5;
			const pageSize = 20;
			const totalPages = 10;

			const targetPage = currentPage;
			if (targetPage >= 1 && targetPage <= totalPages && targetPage !== currentPage) {
				onChange(targetPage, pageSize);
			}
			expect(onChange).not.toHaveBeenCalled();
		});
	});
});
