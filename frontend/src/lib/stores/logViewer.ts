import { writable, derived } from 'svelte/store';
import type { PlotConfig } from '$lib/types';

export const activePlots = writable<PlotConfig[]>([]);

export const plottedFields = derived(activePlots, ($plots) => {
  const map = new Map<string, Set<string>>();
  for (const plot of $plots) {
    const fields = map.get(plot.topic) ?? new Set<string>();
    plot.fields.forEach((f) => fields.add(f));
    map.set(plot.topic, fields);
  }
  return map;
});

/** Reorder plots by moving the item at `fromIndex` to `toIndex`. */
export function reorderPlots(fromIndex: number, toIndex: number): void {
  if (fromIndex === toIndex) return;
  activePlots.update((plots) => {
    const updated = [...plots];
    const [moved] = updated.splice(fromIndex, 1);
    updated.splice(toIndex, 0, moved);
    return updated;
  });
}

/** Toggle the minimized state of a plot by id. */
export function togglePlotMinimized(plotId: string): void {
  activePlots.update((plots) =>
    plots.map((p) => (p.id === plotId ? { ...p, minimized: !p.minimized } : p))
  );
}

/** When set, the plot builder opens in "edit" mode for this SQL plot,
 *  loading its SQL back into the editor. Cleared once consumed. */
export const editSqlPlot = writable<Pick<PlotConfig, 'id' | 'sql' | 'yLabel'> | null>(null);

export const sidebarCollapsed = writable(false);
export const activePanel = writable<'plots' | 'map' | 'messages' | 'params'>('plots');
export const builderOpen = writable(false);
