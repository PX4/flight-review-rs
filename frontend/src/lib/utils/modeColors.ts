/**
 * PX4 main_state / commander_state mode IDs.
 * These are the actual numeric values from vehicle_status.nav_state.
 */
const MODE_COLORS: Record<number, string> = {
  0:  '#4CAF50',  // Manual
  1:  '#8BC34A',  // Altitude
  2:  '#2196F3',  // Position
  3:  '#9C27B0',  // Mission
  4:  '#FF9800',  // Loiter
  5:  '#F44336',  // RTL
  10: '#E91E63',  // Acro
  12: '#FF5722',  // Descend
  13: '#B71C1C',  // Terminate
  14: '#607D8B',  // Offboard
  15: '#00BCD4',  // Stabilized
  17: '#CDDC39',  // Takeoff
  18: '#795548',  // Land
  19: '#FFC107',  // Follow
  20: '#009688',  // Precision Land
  21: '#3F51B5',  // Orbit
  22: '#CDDC39',  // VTOL Takeoff
};

const MODE_NAMES: Record<number, string> = {
  0:  'Manual',
  1:  'Altitude',
  2:  'Position',
  3:  'Mission',
  4:  'Loiter',
  5:  'RTL',
  10: 'Acro',
  12: 'Descend',
  13: 'Terminate',
  14: 'Offboard',
  15: 'Stabilized',
  17: 'Takeoff',
  18: 'Land',
  19: 'Follow',
  20: 'Precision Land',
  21: 'Orbit',
  22: 'VTOL Takeoff',
};

export function getModeColor(id: number): string {
  return MODE_COLORS[id] ?? '#9E9E9E';
}

export function getModeName(id: number): string {
  return MODE_NAMES[id] ?? `Mode ${id}`;
}
