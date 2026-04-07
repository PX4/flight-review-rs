<script lang="ts">
	import { getContext, onMount, onDestroy } from 'svelte';
	import type { FlightMetadata, PlotConfig } from '$lib/types';
	import { activePlots } from '$lib/stores/logViewer';
	import { savePlotLayout, loadPlotLayout } from '$lib/utils/plotPersistence';
	import DragDropPlotList from '$lib/components/viewer/DragDropPlotList.svelte';

	const ctx = getContext<{ metadata: FlightMetadata; logId: string }>('log-viewer');

	const COLORS = ['#818cf8', '#fbbf24', '#34d399', '#f87171', '#a78bfa', '#fb923c', '#38bdf8', '#e879f9'];

	/**
	 * Default plot definitions inspired by PX4 Flight Review v1.
	 * Each entry: [topic, multiId, fields[], yLabel].
	 * Only topics present in the log will be shown.
	 */
	// Default plot definitions matching PX4 Flight Review v1.
	// [id, topic, multiId, fields[], yLabel]
	// Only topics present in the log will be shown.
	const DEFAULT_PLOTS: [string, string, number, string[], string][] = [
		// --- Position & Altitude ---
		['altitude', 'vehicle_local_position', 0, ['z'], 'Altitude (m, NED down)'],
		['local_pos_xy', 'vehicle_local_position', 0, ['x', 'y'], 'Local Position XY (m)'],
		['velocity', 'vehicle_local_position', 0, ['vx', 'vy', 'vz'], 'Velocity (m/s)'],

		// --- Angular Rates ---
		['angular_vel', 'vehicle_angular_velocity', 0, ['xyz[0]', 'xyz[1]', 'xyz[2]'], 'Angular Velocity (rad/s)'],
		['rate_setpoint', 'vehicle_rates_setpoint', 0, ['roll', 'pitch', 'yaw'], 'Rate Setpoint (rad/s)'],
		['rate_ctrl', 'rate_ctrl_status', 0, ['rollspeed_integ', 'pitchspeed_integ', 'yawspeed_integ'], 'Rate Controller Integral'],

		// --- Sensors ---
		['accel', 'sensor_combined', 0, ['accelerometer_m_s2[0]', 'accelerometer_m_s2[1]', 'accelerometer_m_s2[2]'], 'Acceleration (m/s²)'],
		['gyro', 'sensor_combined', 0, ['gyro_rad[0]', 'gyro_rad[1]', 'gyro_rad[2]'], 'Gyroscope (rad/s)'],
		['mag', 'vehicle_magnetometer', 0, ['magnetometer_ga[0]', 'magnetometer_ga[1]', 'magnetometer_ga[2]'], 'Magnetometer (gauss)'],
		['baro', 'sensor_baro', 0, ['pressure', 'temperature'], 'Barometer'],

		// --- Actuators ---
		['actuator_motors', 'actuator_motors', 0, ['control[0]', 'control[1]', 'control[2]', 'control[3]'], 'Motor Outputs'],
		['actuator_servos', 'actuator_servos', 0, ['control[0]', 'control[1]', 'control[2]', 'control[3]'], 'Servo Outputs'],
		['actuator_outputs', 'actuator_outputs', 0, ['output[0]', 'output[1]', 'output[2]', 'output[3]'], 'Raw PWM Outputs'],

		// --- Manual Control ---
		['manual_ctrl', 'manual_control_setpoint', 0, ['roll', 'pitch', 'yaw', 'throttle'], 'Manual Control (sticks)'],

		// --- RC Input ---
		['rc_quality', 'input_rc', 0, ['rssi', 'link_quality'], 'RC Link Quality'],

		// --- Airspeed (fixed-wing / VTOL) ---
		['airspeed', 'airspeed_validated', 0, ['true_airspeed_m_s', 'indicated_airspeed_m_s'], 'Airspeed (m/s)'],

		// --- GPS ---
		['gps_accuracy', 'vehicle_gps_position', 0, ['eph', 'epv'], 'GPS Accuracy (m)'],
		['gps_sats', 'vehicle_gps_position', 0, ['satellites_used'], 'GPS Satellites'],

		// --- Estimator ---
		['est_accuracy', 'estimator_status', 0, ['pos_horiz_accuracy', 'pos_vert_accuracy'], 'Estimator Accuracy (m)'],
		['est_test_ratio', 'estimator_status', 0, ['pos_test_ratio', 'vel_test_ratio', 'hgt_test_ratio'], 'Estimator Test Ratios'],

		// --- Battery & Power ---
		['battery', 'battery_status', 0, ['voltage_v', 'current_a'], 'Battery'],
		['battery_energy', 'battery_status', 0, ['discharged_mah', 'remaining'], 'Battery State'],
		['sys_power', 'system_power', 0, ['voltage5v_v', 'sensors3v3[0]'], 'System Power (V)'],

		// --- VTOL ---
		['vtol', 'vtol_vehicle_status', 0, ['vehicle_vtol_state'], 'VTOL State'],

		// --- System ---
		['cpu', 'cpuload', 0, ['load', 'ram_usage'], 'CPU Load'],
	];

	const TRAJECTORY_PLOT_ID = 'trajectory_2d';

	function makeTrajectoryPlot(): PlotConfig {
		return {
			id: TRAJECTORY_PLOT_ID,
			topic: 'vehicle_local_position',
			multiId: 0,
			fields: [],
			yLabel: '2D Trajectory',
			colors: [],
			kind: 'xy',
		};
	}

	function buildDefaultPlots(metadata: FlightMetadata): PlotConfig[] {
		const availableTopics = new Set(Object.keys(metadata.topics));
		const plots: PlotConfig[] = [];

		// Trajectory plot is always first when local position data is available.
		if (availableTopics.has('vehicle_local_position')) {
			plots.push(makeTrajectoryPlot());
		}

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

	let unsubscribe: (() => void) | undefined;

	onMount(() => {
		if ($activePlots.length === 0) {
			// Try restoring from localStorage, filtering stale topics
			const saved = loadPlotLayout(ctx.logId);
			const availableTopics = new Set(Object.keys(ctx.metadata.topics));
			if (saved && saved.length > 0) {
				const valid = saved.filter((p) => availableTopics.has(p.topic));
				if (valid.length > 0) {
					// Prepend the trajectory plot if missing from saved layout (so existing
					// users pick up the new default the first time they reopen a log).
					if (
						availableTopics.has('vehicle_local_position') &&
						!valid.some((p) => p.id === TRAJECTORY_PLOT_ID)
					) {
						valid.unshift(makeTrajectoryPlot());
					}
					activePlots.set(valid);
				} else {
					activePlots.set(buildDefaultPlots(ctx.metadata));
				}
			} else {
				activePlots.set(buildDefaultPlots(ctx.metadata));
			}
		}

		// Persist plot layout on changes (debounced inside savePlotLayout)
		unsubscribe = activePlots.subscribe((plots) => {
			savePlotLayout(ctx.logId, plots);
		});
	});

	onDestroy(() => {
		unsubscribe?.();
	});
</script>

{#if $activePlots.length > 0}
	<DragDropPlotList plots={$activePlots} logId={ctx.logId} metadata={ctx.metadata} />
{:else}
	<div class="flex flex-col items-center justify-center py-24 text-center">
		<svg class="size-12 text-gray-300 mb-4" fill="none" viewBox="0 0 24 24" stroke-width="1" stroke="currentColor">
			<path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z" />
		</svg>
		<h3 class="text-sm font-medium text-gray-900">No plots selected</h3>
		<p class="mt-1 text-sm text-gray-500">Click "Add Plot" above to select topics and fields to plot.</p>
	</div>
{/if}
