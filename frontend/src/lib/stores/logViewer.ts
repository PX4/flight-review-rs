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

export const sidebarCollapsed = writable(false);
export const activePanel = writable<'plots' | 'map' | 'messages' | 'params'>('plots');
