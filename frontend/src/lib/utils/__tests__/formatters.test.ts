import { describe, it, expect, vi } from 'vitest';
import { formatDuration, formatFileSize, formatRelativeTime } from '../formatters';

describe('formatDuration', () => {
	it('returns dash for null', () => {
		expect(formatDuration(null)).toBe('\u2014');
	});

	it('formats seconds only', () => {
		expect(formatDuration(45)).toBe('45s');
	});

	it('formats minutes and seconds', () => {
		expect(formatDuration(754)).toBe('12m 34s');
	});

	it('formats hours and minutes', () => {
		expect(formatDuration(3661)).toBe('1h 1m');
	});

	it('formats zero seconds', () => {
		expect(formatDuration(0)).toBe('0s');
	});

	it('formats exactly 60 seconds as minutes', () => {
		expect(formatDuration(60)).toBe('1m 0s');
	});

	it('formats exactly 3600 seconds as hours', () => {
		expect(formatDuration(3600)).toBe('1h 0m');
	});
});

describe('formatFileSize', () => {
	it('formats bytes', () => {
		expect(formatFileSize(500)).toBe('500 B');
	});

	it('formats kilobytes', () => {
		expect(formatFileSize(1536)).toBe('1.5 KB');
	});

	it('formats megabytes', () => {
		expect(formatFileSize(5242880)).toBe('5.0 MB');
	});

	it('formats gigabytes', () => {
		expect(formatFileSize(1073741824)).toBe('1.0 GB');
	});

	it('formats zero bytes', () => {
		expect(formatFileSize(0)).toBe('0 B');
	});
});

describe('formatRelativeTime', () => {
	it('returns "just now" for recent timestamps', () => {
		const now = new Date().toISOString();
		expect(formatRelativeTime(now)).toBe('just now');
	});

	it('returns minutes ago', () => {
		const date = new Date(Date.now() - 5 * 60 * 1000).toISOString();
		expect(formatRelativeTime(date)).toBe('5m ago');
	});

	it('returns hours ago', () => {
		const date = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
		expect(formatRelativeTime(date)).toBe('3h ago');
	});

	it('returns days ago', () => {
		const date = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString();
		expect(formatRelativeTime(date)).toBe('7d ago');
	});

	it('returns date string for old timestamps', () => {
		const date = new Date(Date.now() - 60 * 24 * 60 * 60 * 1000).toISOString();
		const result = formatRelativeTime(date);
		// Should be a locale date string, not a relative time
		expect(result).not.toContain('ago');
		expect(result).not.toBe('just now');
	});
});
