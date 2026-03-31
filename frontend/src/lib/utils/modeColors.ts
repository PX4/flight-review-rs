const MODE_COLORS: Record<number, string> = {
  0: '#4CAF50',   // Manual
  1: '#8BC34A',   // Altitude
  2: '#2196F3',   // Position
  3: '#9C27B0',   // Mission
  4: '#FF9800',   // Loiter
  5: '#F44336',   // RTL
  6: '#00BCD4',   // Stabilized
  7: '#E91E63',   // Acro
  8: '#607D8B',   // Offboard
  9: '#CDDC39',   // Takeoff
  10: '#795548',  // Land
  11: '#3F51B5',  // Orbit
};

const MODE_NAMES: Record<number, string> = {
  0: 'Manual',
  1: 'Altitude',
  2: 'Position',
  3: 'Mission',
  4: 'Loiter',
  5: 'RTL',
  6: 'Stabilized',
  7: 'Acro',
  8: 'Offboard',
  9: 'Takeoff',
  10: 'Land',
  11: 'Orbit',
};

export function getModeColor(id: number): string {
  return MODE_COLORS[id] ?? '#9E9E9E';
}

export function getModeName(id: number): string {
  return MODE_NAMES[id] ?? `Mode ${id}`;
}
