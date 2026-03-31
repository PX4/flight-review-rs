import { writable } from 'svelte/store';

export const timeRange = writable<[number, number] | null>(null);
export const cursorTimestamp = writable<number | null>(null);
export const SYNC_KEY = 'flight-review-sync';
