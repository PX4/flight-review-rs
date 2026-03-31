import { describe, it, expect } from 'vitest';
import type { ParamDiff, ChangedParam } from '$lib/types';

/**
 * ParamDiffPanel logic tests.
 * Tests sorting, searching, and delta coloring without rendering the Svelte component.
 */

type SortKey = 'name' | 'value' | 'default' | 'delta';
type SortDir = 'asc' | 'desc';

function getDeltaPct(diff: ParamDiff): number {
	if (diff.default === 0) return diff.value === 0 ? 0 : 100;
	return Math.abs((diff.value - diff.default) / diff.default) * 100;
}

function getDeltaColor(pct: number): string {
	if (pct > 50) return 'text-red-600';
	if (pct > 20) return 'text-orange-500';
	return 'text-green-600';
}

function filterAndSort(
	diffs: ParamDiff[],
	inFlightNames: Set<string>,
	searchText: string,
	showOnlyInFlight: boolean,
	sortKey: SortKey,
	sortDir: SortDir
): ParamDiff[] {
	const query = searchText.toLowerCase();
	let result = diffs;

	if (showOnlyInFlight) {
		result = result.filter((d) => inFlightNames.has(d.name));
	}

	if (query) {
		result = result.filter((d) => d.name.toLowerCase().includes(query));
	}

	const dir = sortDir === 'asc' ? 1 : -1;
	result = [...result].sort((a, b) => {
		switch (sortKey) {
			case 'name':
				return dir * a.name.localeCompare(b.name);
			case 'value':
				return dir * (a.value - b.value);
			case 'default':
				return dir * (a.default - b.default);
			case 'delta':
				return dir * (getDeltaPct(a) - getDeltaPct(b));
			default:
				return 0;
		}
	});

	return result;
}

const sampleDiffs: ParamDiff[] = [
	{ name: 'MC_PITCHRATE_P', value: 0.15, default: 0.1 },      // 50%
	{ name: 'MC_ROLLRATE_P', value: 0.08, default: 0.1 },        // 20%
	{ name: 'EKF2_GPS_DELAY', value: 200, default: 110 },        // ~81.8%
	{ name: 'BAT_LOW_THR', value: 0.15, default: 0.15 },         // 0%
	{ name: 'MPC_XY_VEL_MAX', value: 12, default: 8 },           // 50%
	{ name: 'ZERO_DEFAULT', value: 5, default: 0 },              // 100% (div by zero)
];

const sampleChangedParams: ChangedParam[] = [
	{ name: 'MC_PITCHRATE_P', value: 0.15, in_flight: true },
	{ name: 'BAT_LOW_THR', value: 0.15, in_flight: false },
	{ name: 'EKF2_GPS_DELAY', value: 200, in_flight: true },
];

const inFlightNames = new Set(
	sampleChangedParams.filter((c) => c.in_flight).map((c) => c.name)
);

describe('ParamDiffPanel', () => {
	describe('delta percentage calculation', () => {
		it('calculates correct delta for increased value', () => {
			const pct = getDeltaPct(sampleDiffs[0]); // 0.15 vs 0.1
			expect(pct).toBeCloseTo(50);
		});

		it('calculates correct delta for decreased value', () => {
			const pct = getDeltaPct(sampleDiffs[1]); // 0.08 vs 0.1
			expect(pct).toBeCloseTo(20);
		});

		it('calculates 0% for unchanged value', () => {
			const pct = getDeltaPct(sampleDiffs[3]); // 0.15 vs 0.15
			expect(pct).toBeCloseTo(0);
		});

		it('handles zero default (non-zero value)', () => {
			const pct = getDeltaPct(sampleDiffs[5]); // 5 vs 0
			expect(pct).toBe(100);
		});

		it('handles both zero', () => {
			const pct = getDeltaPct({ name: 'X', value: 0, default: 0 });
			expect(pct).toBe(0);
		});
	});

	describe('delta coloring', () => {
		it('returns green for small delta (<= 20%)', () => {
			expect(getDeltaColor(0)).toBe('text-green-600');
			expect(getDeltaColor(10)).toBe('text-green-600');
			expect(getDeltaColor(20)).toBe('text-green-600');
		});

		it('returns orange for moderate delta (> 20%, <= 50%)', () => {
			expect(getDeltaColor(21)).toBe('text-orange-500');
			expect(getDeltaColor(35)).toBe('text-orange-500');
			expect(getDeltaColor(50)).toBe('text-orange-500');
		});

		it('returns red for large delta (> 50%)', () => {
			expect(getDeltaColor(51)).toBe('text-red-600');
			expect(getDeltaColor(100)).toBe('text-red-600');
		});
	});

	describe('sorting', () => {
		it('sorts by name ascending', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'name', 'asc');
			expect(result[0].name).toBe('BAT_LOW_THR');
			expect(result[result.length - 1].name).toBe('ZERO_DEFAULT');
		});

		it('sorts by name descending', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'name', 'desc');
			expect(result[0].name).toBe('ZERO_DEFAULT');
			expect(result[result.length - 1].name).toBe('BAT_LOW_THR');
		});

		it('sorts by delta ascending', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'delta', 'asc');
			expect(getDeltaPct(result[0])).toBeCloseTo(0);
		});

		it('sorts by delta descending', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'delta', 'desc');
			expect(getDeltaPct(result[0])).toBe(100); // ZERO_DEFAULT
		});

		it('sorts by value ascending', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'value', 'asc');
			expect(result[0].value).toBe(0.08);
		});

		it('sorts by default ascending', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'default', 'asc');
			expect(result[0].default).toBe(0);
		});
	});

	describe('search filtering', () => {
		it('filters by param name case-insensitively', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, 'mc_', false, 'name', 'asc');
			expect(result).toHaveLength(2);
			expect(result.every((d) => d.name.startsWith('MC_'))).toBe(true);
		});

		it('returns empty for unmatched search', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, 'nonexistent', false, 'name', 'asc');
			expect(result).toHaveLength(0);
		});

		it('returns all when search is empty', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', false, 'name', 'asc');
			expect(result).toHaveLength(6);
		});
	});

	describe('in-flight filter', () => {
		it('shows only in-flight changed params', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, '', true, 'name', 'asc');
			expect(result).toHaveLength(2);
			expect(result.map((d) => d.name).sort()).toEqual(['EKF2_GPS_DELAY', 'MC_PITCHRATE_P']);
		});

		it('combines in-flight filter with search', () => {
			const result = filterAndSort(sampleDiffs, inFlightNames, 'ekf', true, 'name', 'asc');
			expect(result).toHaveLength(1);
			expect(result[0].name).toBe('EKF2_GPS_DELAY');
		});
	});
});
