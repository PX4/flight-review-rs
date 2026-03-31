import { describe, it, expect } from 'vitest';
import { getHardwareName } from '../hardwareNames';

describe('getHardwareName', () => {
	it('maps known board identifiers to pretty names', () => {
		expect(getHardwareName('CUBEPILOT_CUBEORANGEPLUS')).toBe('Cube Orange+');
		expect(getHardwareName('PX4_FMU_V6X')).toBe('Pixhawk 6X');
		expect(getHardwareName('PX4_FMU_V5')).toBe('Pixhawk 4');
		expect(getHardwareName('CUAV_V5PLUS')).toBe('CUAV V5+');
		expect(getHardwareName('PX4_SITL')).toBe('SITL (Simulation)');
	});

	it('is case-insensitive', () => {
		expect(getHardwareName('cubepilot_cubeorangeplus')).toBe('Cube Orange+');
		expect(getHardwareName('px4_fmu_v6c')).toBe('Pixhawk 6C');
	});

	it('cleans up unknown board names', () => {
		const result = getHardwareName('SOME_UNKNOWN_BOARD_V2');
		expect(result).toBe('Some Unknown Board V2');
	});

	it('returns Unknown for null/undefined', () => {
		expect(getHardwareName(null)).toBe('Unknown');
		expect(getHardwareName(undefined)).toBe('Unknown');
	});

	it('handles partial matches in raw string', () => {
		expect(getHardwareName('HOLYBRO_PIXHAWK6X_V2')).toBe('Pixhawk 6X');
	});
});
