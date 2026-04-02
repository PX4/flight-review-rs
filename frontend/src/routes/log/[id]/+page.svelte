<script lang="ts">
	import { getContext, onMount } from 'svelte';
	import type { FlightMetadata, PlotConfig } from '$lib/types';
	import { activePlots } from '$lib/stores/logViewer';
	import PlotStrip from '$lib/components/viewer/PlotStrip.svelte';

	const ctx = getContext<{ metadata: FlightMetadata; logId: string }>('log-viewer');

	const COLORS = ['#818cf8', '#fbbf24', '#34d399', '#f87171', '#a78bfa', '#fb923c', '#38bdf8', '#e879f9'];

	/**
	 * Default plot definitions inspired by PX4 Flight Review v1.
	 * Each entry: [topic, multiId, fields[], yLabel].
	 * Only topics present in the log will be shown.
	 */
	const DEFAULT_PLOTS: [string, string, number, string[], string][] = [
		// [id, topic, multiId, fields, yLabel]
		// Altitude (NED: z is down, so negate for display — handled by user mentally)
		['altitude', 'vehicle_local_position', 0, ['z'], 'Altitude (m, NED down)'],
		// Velocity
		['velocity', 'vehicle_local_position', 0, ['vx', 'vy', 'vz'], 'Velocity (m/s)'],
		// Angular velocity (roll/pitch/yaw rates)
		['angular_vel', 'vehicle_angular_velocity', 0, ['xyz[0]', 'xyz[1]', 'xyz[2]'], 'Angular Velocity (rad/s)'],
		// Accelerometer
		['accel', 'sensor_combined', 0, ['accelerometer_m_s2[0]', 'accelerometer_m_s2[1]', 'accelerometer_m_s2[2]'], 'Acceleration (m/s²)'],
		// Actuator outputs
		['actuators', 'actuator_outputs', 0, ['output[0]', 'output[1]', 'output[2]', 'output[3]'], 'Actuator Outputs'],
		// Battery
		['battery', 'battery_status', 0, ['voltage_v', 'current_a'], 'Battery'],
		// CPU load
		['cpu', 'cpuload', 0, ['load', 'ram_usage'], 'CPU Load'],
	];

	function buildDefaultPlots(metadata: FlightMetadata): PlotConfig[] {
		const availableTopics = new Set(Object.keys(metadata.topics));
		const plots: PlotConfig[] = [];

		for (const [id, topic, multiId, fields, yLabel] of DEFAULT_PLOTS) {
			if (!availableTopics.has(topic)) continue;

			plots.push({
				id,
				topic,
				multiId,
				fields,
				yLabel,
				colors: fields.map((_, i) => COLORS[i % COLORS.length]),
			});
		}

		return plots;
	}

	onMount(() => {
		// Only set defaults if no plots are already active (user may have navigated back)
		if ($activePlots.length === 0) {
			activePlots.set(buildDefaultPlots(ctx.metadata));
		}
	});
</script>

{#if $activePlots.length > 0}
	{#each $activePlots as plot (plot.id)}
		<PlotStrip config={plot} logId={ctx.logId} metadata={ctx.metadata} />
	{/each}
{:else}
	<div class="flex flex-col items-center justify-center py-24 text-center">
		<svg class="size-12 text-gray-300 mb-4" fill="none" viewBox="0 0 24 24" stroke-width="1" stroke="currentColor">
			<path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z" />
		</svg>
		<h3 class="text-sm font-medium text-gray-900">No plots selected</h3>
		<p class="mt-1 text-sm text-gray-500">Expand a topic in the sidebar and select fields to plot.</p>
	</div>
{/if}
