/**
 * uPlot touch-zoom plugin — pinch-to-zoom and single-finger pan on the x-axis,
 * plus Shift+scroll to zoom and Shift+drag to pan on desktop.
 *
 * Adapted from the official uPlot zoom-touch demo by Leon Sorokin:
 * https://github.com/leeoniya/uPlot/blob/master/demos/zoom-touch.html
 *
 * Changes from original:
 * - X-axis only (y-axis is left untouched)
 * - TypeScript types
 * - Desktop: Shift+wheel zoom, Shift+drag pan
 * - Cleanup on destroy
 */

import type uPlot from 'uplot';

interface Pos {
	x: number;
	y: number;
	d: number;
	dx: number;
	dy: number;
}

export function touchZoomPlugin(): uPlot.Plugin {
	function init(u: uPlot) {
		const over = u.over;
		let rect: DOMRect;
		let oxRange: number;
		let xVal: number;

		const fr: Pos = { x: 0, y: 0, d: 1, dx: 0, dy: 0 };
		const to: Pos = { x: 0, y: 0, d: 1, dx: 0, dy: 0 };

		function storePos(t: Pos, e: TouchEvent) {
			const ts = e.touches;
			const t0 = ts[0];
			const t0x = t0.clientX - rect.left;
			const t0y = t0.clientY - rect.top;

			if (ts.length === 1) {
				t.x = t0x;
				t.y = t0y;
				t.d = t.dx = t.dy = 1;
			} else {
				const t1 = ts[1];
				const t1x = t1.clientX - rect.left;
				const t1y = t1.clientY - rect.top;

				const xMin = Math.min(t0x, t1x);
				const xMax = Math.max(t0x, t1x);
				const yMin = Math.min(t0y, t1y);
				const yMax = Math.max(t0y, t1y);

				t.x = (xMin + xMax) / 2;
				t.y = (yMin + yMax) / 2;
				t.dx = xMax - xMin;
				t.dy = yMax - yMin;
				t.d = Math.sqrt(t.dx * t.dx + t.dy * t.dy);
			}
		}

		// --- Touch: pinch-to-zoom and single-finger pan ---

		let rafPending = false;

		function zoom() {
			rafPending = false;

			const left = to.x;
			const xFactor = fr.d / to.d;
			const leftPct = left / rect.width;

			const nxRange = oxRange * xFactor;
			const nxMin = xVal - leftPct * nxRange;
			const nxMax = nxMin + nxRange;

			u.setScale('x', { min: nxMin, max: nxMax });
		}

		function touchmove(e: TouchEvent) {
			storePos(to, e);

			if (!rafPending) {
				rafPending = true;
				requestAnimationFrame(zoom);
			}
		}

		function touchstart(e: TouchEvent) {
			rect = over.getBoundingClientRect();
			storePos(fr, e);

			oxRange = (u.scales.x.max ?? 0) - (u.scales.x.min ?? 0);
			xVal = u.posToVal(fr.x, 'x');

			document.addEventListener('touchmove', touchmove, { passive: true });
		}

		function touchend() {
			document.removeEventListener('touchmove', touchmove);
		}

		over.addEventListener('touchstart', touchstart, { passive: true });
		over.addEventListener('touchend', touchend);

		// --- Desktop: Shift+wheel to zoom ---

		let wheelRafPending = false;
		let wheelAccum = 0;
		let wheelCursorPct = 0.5;

		function applyWheelZoom() {
			wheelRafPending = false;

			const xRange = (u.scales.x.max ?? 0) - (u.scales.x.min ?? 0);
			// Each ~100px of wheel delta = 20% zoom
			const factor = Math.pow(1.2, wheelAccum / 100);
			wheelAccum = 0;

			const nxRange = xRange * factor;
			const xMin = u.scales.x.min ?? 0;
			// Keep the point under the cursor fixed
			const nxMin = xMin + (xRange - nxRange) * wheelCursorPct;
			const nxMax = nxMin + nxRange;

			u.setScale('x', { min: nxMin, max: nxMax });
		}

		function onWheel(e: WheelEvent) {
			if (!e.shiftKey) return;

			e.preventDefault();

			rect = over.getBoundingClientRect();
			wheelCursorPct = (e.clientX - rect.left) / rect.width;
			// macOS swaps deltaY to deltaX when Shift is held
			const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
			wheelAccum += delta;

			if (!wheelRafPending) {
				wheelRafPending = true;
				requestAnimationFrame(applyWheelZoom);
			}
		}

		over.addEventListener('wheel', onWheel, { passive: false });

		// --- Desktop: Shift+drag to pan ---

		let dragging = false;
		let dragStartX = 0;
		let dragOxMin = 0;
		let dragOxMax = 0;
		let dragRafPending = false;
		let dragCurrentX = 0;

		function applyDragPan() {
			dragRafPending = false;

			rect = over.getBoundingClientRect();
			const dx = dragCurrentX - dragStartX;
			const xRange = dragOxMax - dragOxMin;
			const shift = -(dx / rect.width) * xRange;

			u.setScale('x', { min: dragOxMin + shift, max: dragOxMax + shift });
		}

		function onMouseDown(e: MouseEvent) {
			if (!e.shiftKey || e.button !== 0) return;

			e.preventDefault();
			// Suppress uPlot's built-in cursor drag while we're panning
			over.style.pointerEvents = 'none';

			dragging = true;
			dragStartX = e.clientX;
			dragOxMin = u.scales.x.min ?? 0;
			dragOxMax = u.scales.x.max ?? 0;

			document.addEventListener('mousemove', onMouseMove);
			document.addEventListener('mouseup', onMouseUp);
		}

		function onMouseMove(e: MouseEvent) {
			if (!dragging) return;

			dragCurrentX = e.clientX;
			if (!dragRafPending) {
				dragRafPending = true;
				requestAnimationFrame(applyDragPan);
			}
		}

		function onMouseUp() {
			dragging = false;
			over.style.pointerEvents = '';
			document.removeEventListener('mousemove', onMouseMove);
			document.removeEventListener('mouseup', onMouseUp);
		}

		over.addEventListener('mousedown', onMouseDown);

		// Store refs for cleanup
		(u as any)._touchZoom = { touchstart, touchend, onWheel, onMouseDown };
	}

	function destroy(u: uPlot) {
		const refs = (u as any)._touchZoom;
		if (refs) {
			u.over.removeEventListener('touchstart', refs.touchstart);
			u.over.removeEventListener('touchend', refs.touchend);
			u.over.removeEventListener('wheel', refs.onWheel);
			u.over.removeEventListener('mousedown', refs.onMouseDown);
		}
	}

	return {
		hooks: {
			init: [init],
			destroy: [destroy],
		},
	};
}
