import { describe, it, expect } from 'vitest';
import { buildQgcParamsFile } from '../paramExport';

describe('buildQgcParamsFile', () => {
	it('skips parameters the log stored as nonfinite (null)', () => {
		const content = buildQgcParamsFile({ BAT_A_PER_V: null, BAT_N_CELLS: 4, MPC_XY_P: 0.95 });
		const rows = content.split('\n').filter((line) => line && !line.startsWith('#'));
		expect(rows).toEqual(['1\t1\tBAT_N_CELLS\t4.0\t9', '1\t1\tMPC_XY_P\t0.95\t9']);
	});
});
