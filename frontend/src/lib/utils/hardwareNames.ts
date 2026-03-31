/**
 * Maps raw PX4 ver_hw board identifiers to human-readable product names.
 * These come from the board_config.h BOARD_NAME defines in PX4-Autopilot.
 */
const HARDWARE_NAMES: Record<string, string> = {
	// Pixhawk series (Holybro)
	'PX4_FMU_V2': 'Pixhawk 1',
	'PX4_FMU_V3': 'Pixhawk 2 (Cube)',
	'PX4_FMU_V4': 'Pixhawk 3 Pro',
	'PX4_FMU_V4PRO': 'Pixhawk 3 Pro',
	'PX4_FMU_V5': 'Pixhawk 4',
	'PX4_FMU_V5X': 'Pixhawk 5X',
	'PX4_FMU_V6C': 'Pixhawk 6C',
	'PX4_FMU_V6X': 'Pixhawk 6X',
	'PX4_FMU_V6U': 'Pixhawk 6U',
	'HOLYBRO_PIXHAWK5X': 'Pixhawk 5X',
	'HOLYBRO_PIXHAWK6X': 'Pixhawk 6X',
	'HOLYBRO_PIXHAWK6C': 'Pixhawk 6C',

	// CubePilot
	'CUBEPILOT_CUBEORANGE': 'Cube Orange',
	'CUBEPILOT_CUBEORANGEPLUS': 'Cube Orange+',
	'CUBEPILOT_CUBEYELLOW': 'Cube Yellow',
	'CUBEPILOT_CUBEBLACK': 'Cube Black',
	'CUBEPILOT_CUBEBLACKPLUS': 'Cube Black+',

	// mRo
	'MRO_X2V10': 'mRo X2.1',
	'MRO_CTRL_ZERO_F7': 'mRo Control Zero F7',
	'MRO_CTRL_ZERO_H7': 'mRo Control Zero H7',
	'MRO_CTRL_ZERO_H7_OEM': 'mRo Control Zero H7 OEM',
	'MROV2': 'mRo Pixhawk',
	'MRO_PIXRACERPRO': 'mRo Pixracer Pro',

	// CUAV
	'CUAV_X7PRO': 'CUAV X7 Pro',
	'CUAV_X7PROBASE': 'CUAV X7 Pro Base',
	'CUAV_NORA': 'CUAV Nora',
	'CUAV_V5PLUS': 'CUAV V5+',
	'CUAV_V5NANO': 'CUAV V5 Nano',

	// Auterion
	'SKYNODE': 'Auterion Skynode',
	'SKYNODE_X': 'Auterion Skynode X',

	// Diatone
	'DIATONE_MAMBA_F405_MK2': 'Diatone Mamba F405 MK2',

	// ModalAI
	'MODALAI_FC_V1': 'ModalAI FC v1',
	'MODALAI_FC_V2': 'ModalAI FC v2',
	'MODALAI_VOXL2': 'ModalAI VOXL 2',

	// ARK
	'ARK_FMU_V6X': 'ARK FPV FC',
	'ARKV6X': 'ARK FMU V6X',

	// Hex/ProfiCNC
	'HEXA_HERE_FLOW': 'Here Flow',

	// NXP
	'NXP_FMUK66_V3': 'NXP FMUK66',
	'NXP_FMURT1062': 'NXP FMURT1062',

	// Raspberry Pi
	'RPI_PICO': 'Raspberry Pi Pico',

	// SITL / Simulation
	'PX4_SITL': 'SITL (Simulation)',
};

/**
 * Returns a human-readable hardware name for a raw ver_hw string.
 * Falls back to cleaning up the raw string if no mapping exists.
 */
export function getHardwareName(verHw: string | null | undefined): string {
	if (!verHw) return 'Unknown';

	// Check exact match
	const upper = verHw.toUpperCase();
	if (HARDWARE_NAMES[upper]) return HARDWARE_NAMES[upper];

	// Check if raw string contains a known key (handles prefixed variants)
	for (const [key, name] of Object.entries(HARDWARE_NAMES)) {
		if (upper.includes(key)) return name;
	}

	// Fallback: clean up the raw string for display
	return verHw
		.replace(/_/g, ' ')
		.toLowerCase()
		.replace(/\b\w/g, (c) => c.toUpperCase())
		.trim();
}
