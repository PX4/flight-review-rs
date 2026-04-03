import { describe, it, expect } from 'vitest';
import type { LogEntry } from '$lib/types';

/**
 * LogMessagesPanel logic tests.
 * Tests filtering by level and search text without rendering the Svelte component.
 */

type Level = 'ERROR' | 'WARNING' | 'INFO' | 'DEBUG';

function normalizeLevel(level: string): Level {
	const upper = level.toUpperCase();
	if (upper === 'ERROR' || upper === 'ERR') return 'ERROR';
	if (upper === 'WARNING' || upper === 'WARN') return 'WARNING';
	if (upper === 'INFO') return 'INFO';
	return 'DEBUG';
}

function filterMessages(
	messages: LogEntry[],
	enabledLevels: Set<Level>,
	searchText: string
): LogEntry[] {
	const query = searchText.toLowerCase();
	return messages.filter((m) => {
		const level = normalizeLevel(m.level);
		if (!enabledLevels.has(level)) return false;
		if (query && !m.message.toLowerCase().includes(query)) return false;
		return true;
	});
}

function formatTimestamp(timestampUs: number, flightStartUs: number): string {
	const relativeUs = timestampUs - flightStartUs;
	const totalMs = Math.max(0, relativeUs / 1000);
	const ms = Math.floor(totalMs % 1000);
	const totalSec = Math.floor(totalMs / 1000);
	const sec = totalSec % 60;
	const totalMin = Math.floor(totalSec / 60);
	const min = totalMin % 60;
	const hr = Math.floor(totalMin / 60);
	return `${String(hr).padStart(2, '0')}:${String(min).padStart(2, '0')}:${String(sec).padStart(2, '0')}.${String(ms).padStart(3, '0')}`;
}

const sampleMessages: LogEntry[] = [
	{ level: 'ERROR', timestamp_us: 1_000_000, message: 'Sensor failure detected' },
	{ level: 'WARNING', timestamp_us: 2_000_000, message: 'Low battery warning' },
	{ level: 'INFO', timestamp_us: 3_000_000, message: 'GPS lock acquired' },
	{ level: 'INFO', timestamp_us: 4_000_000, message: 'Takeoff complete' },
	{ level: 'DEBUG', timestamp_us: 5_000_000, message: 'EKF status good' },
	{ level: 'WARN', timestamp_us: 6_000_000, message: 'High vibration detected' },
	{ level: 'ERR', timestamp_us: 7_000_000, message: 'Compass inconsistency' },
];

describe('LogMessagesPanel', () => {
	describe('level filtering', () => {
		it('filters by ERROR level only', () => {
			const result = filterMessages(sampleMessages, new Set(['ERROR']), '');
			expect(result).toHaveLength(2); // ERROR + ERR
			expect(result.every((m) => normalizeLevel(m.level) === 'ERROR')).toBe(true);
		});

		it('filters by multiple levels', () => {
			const result = filterMessages(sampleMessages, new Set(['ERROR', 'WARNING']), '');
			expect(result).toHaveLength(4); // 2 errors + 2 warnings
		});

		it('returns empty for no enabled levels', () => {
			const result = filterMessages(sampleMessages, new Set(), '');
			expect(result).toHaveLength(0);
		});

		it('returns all messages when all levels enabled', () => {
			const result = filterMessages(
				sampleMessages,
				new Set(['ERROR', 'WARNING', 'INFO', 'DEBUG']),
				''
			);
			expect(result).toHaveLength(7);
		});

		it('normalizes ERR to ERROR', () => {
			const result = filterMessages(sampleMessages, new Set(['ERROR']), '');
			const errMessages = result.filter((m) => m.level === 'ERR');
			expect(errMessages).toHaveLength(1);
		});

		it('normalizes WARN to WARNING', () => {
			const result = filterMessages(sampleMessages, new Set(['WARNING']), '');
			const warnMessages = result.filter((m) => m.level === 'WARN');
			expect(warnMessages).toHaveLength(1);
		});
	});

	describe('search filtering', () => {
		it('filters by search text case-insensitively', () => {
			const all = new Set<Level>(['ERROR', 'WARNING', 'INFO', 'DEBUG']);
			const result = filterMessages(sampleMessages, all, 'gps');
			expect(result).toHaveLength(1);
			expect(result[0].message).toBe('GPS lock acquired');
		});

		it('combines level and search filters', () => {
			const result = filterMessages(sampleMessages, new Set<Level>(['INFO']), 'takeoff');
			expect(result).toHaveLength(1);
			expect(result[0].message).toBe('Takeoff complete');
		});

		it('returns no results for unmatched search', () => {
			const all = new Set<Level>(['ERROR', 'WARNING', 'INFO', 'DEBUG']);
			const result = filterMessages(sampleMessages, all, 'nonexistent');
			expect(result).toHaveLength(0);
		});

		it('returns all when search is empty', () => {
			const all = new Set<Level>(['ERROR', 'WARNING', 'INFO', 'DEBUG']);
			const result = filterMessages(sampleMessages, all, '');
			expect(result).toHaveLength(7);
		});
	});

	describe('timestamp formatting', () => {
		it('formats zero offset correctly', () => {
			expect(formatTimestamp(0, 0)).toBe('00:00:00.000');
		});

		it('formats seconds and milliseconds', () => {
			expect(formatTimestamp(5_500_000, 0)).toBe('00:00:05.500');
		});

		it('formats minutes', () => {
			expect(formatTimestamp(125_000_000, 0)).toBe('00:02:05.000');
		});

		it('formats hours', () => {
			expect(formatTimestamp(3_661_000_000, 0)).toBe('01:01:01.000');
		});

		it('subtracts flight start time', () => {
			expect(formatTimestamp(10_000_000, 5_000_000)).toBe('00:00:05.000');
		});

		it('clamps negative relative time to zero', () => {
			expect(formatTimestamp(1_000_000, 5_000_000)).toBe('00:00:00.000');
		});
	});
});
