import { describe, it, expect } from 'vitest';
import { getModeColor, getModeName } from '../modeColors';

describe('getModeColor', () => {
	it('returns correct color for Manual (0)', () => {
		expect(getModeColor(0)).toBe('#4CAF50');
	});

	it('returns correct color for Position (2)', () => {
		expect(getModeColor(2)).toBe('#2196F3');
	});

	it('returns correct color for Mission (3)', () => {
		expect(getModeColor(3)).toBe('#9C27B0');
	});

	it('returns correct color for RTL (5)', () => {
		expect(getModeColor(5)).toBe('#F44336');
	});

	it('returns correct color for Orbit (11)', () => {
		expect(getModeColor(11)).toBe('#3F51B5');
	});

	it('returns gray for unknown mode id', () => {
		expect(getModeColor(99)).toBe('#9E9E9E');
	});

	it('returns gray for negative mode id', () => {
		expect(getModeColor(-1)).toBe('#9E9E9E');
	});
});

describe('getModeName', () => {
	it('returns correct name for Manual (0)', () => {
		expect(getModeName(0)).toBe('Manual');
	});

	it('returns correct name for Mission (3)', () => {
		expect(getModeName(3)).toBe('Mission');
	});

	it('returns correct name for Offboard (8)', () => {
		expect(getModeName(8)).toBe('Offboard');
	});

	it('returns fallback for unknown mode id', () => {
		expect(getModeName(99)).toBe('Mode 99');
	});
});
