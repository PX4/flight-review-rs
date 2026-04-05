/**
 * uPlot touch-zoom plugin — pinch-to-zoom and single-finger pan on the x-axis.
 *
 * Adapted from the official uPlot zoom-touch demo by Leon Sorokin:
 * https://github.com/leeoniya/uPlot/blob/master/demos/zoom-touch.html
 *
 * Changes from original:
 * - X-axis only (y-axis is left untouched)
 * - TypeScript types
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

		let rafPending = false;

		function zoom() {
			rafPending = false;

			const left = to.x;

			// For single-finger pan: d stays 1, so xFactor = 1 — only the
			// position shift (leftPct difference) moves the viewport.
			// For pinch: d changes, producing a zoom factor.
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

		// Store refs for cleanup
		(u as any)._touchZoom = { touchstart, touchend };
	}

	function destroy(u: uPlot) {
		const refs = (u as any)._touchZoom;
		if (refs) {
			u.over.removeEventListener('touchstart', refs.touchstart);
			u.over.removeEventListener('touchend', refs.touchend);
		}
	}

	return {
		hooks: {
			init: [init],
			destroy: [destroy],
		},
	};
}
