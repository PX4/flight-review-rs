import { writable } from 'svelte/store';

export const timeRange = writable<[number, number] | null>(null);
export const cursorTimestamp = writable<number | null>(null);
export const SYNC_KEY = 'flight-review-sync';

/**
 * Throttled write to timeRange — ensures synced plots update at most
 * once per animation frame (~60fps) instead of on every input event.
 */
let pendingRange: [number, number] | null = null;
let rafId = 0;

export function setTimeRange(min: number, max: number) {
	pendingRange = [min, max];
	if (!rafId) {
		rafId = requestAnimationFrame(() => {
			rafId = 0;
			if (pendingRange) {
				timeRange.set(pendingRange);
				pendingRange = null;
			}
		});
	}
}
