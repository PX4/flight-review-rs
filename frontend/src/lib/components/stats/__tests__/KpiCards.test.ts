import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import KpiCards from '../KpiCards.svelte';

describe('KpiCards', () => {
	it('renders KPI values when not loading', () => {
		render(KpiCards, {
			props: {
				totalLogs: 12345,
				flightHours: 678.9,
				uniqueVehicles: 42,
				todayUploads: 17,
				loading: false,
			},
		});

		expect(screen.getByText('12,345')).toBeTruthy();
		expect(screen.getByText('678.9')).toBeTruthy();
		expect(screen.getByText('42')).toBeTruthy();
		expect(screen.getByText('17')).toBeTruthy();
		expect(screen.getByText('Total Logs')).toBeTruthy();
		expect(screen.getByText('Flight Hours')).toBeTruthy();
		expect(screen.getByText('Unique Vehicles')).toBeTruthy();
		expect(screen.getByText("Today's Uploads")).toBeTruthy();
	});

	it('renders loading skeletons when loading', () => {
		const { container } = render(KpiCards, {
			props: {
				totalLogs: 0,
				flightHours: 0,
				uniqueVehicles: 0,
				todayUploads: 0,
				loading: true,
			},
		});

		const pulseElements = container.querySelectorAll('.animate-pulse');
		expect(pulseElements.length).toBeGreaterThanOrEqual(4);
	});

	it('formats large flight hours with k suffix', () => {
		render(KpiCards, {
			props: {
				totalLogs: 100,
				flightHours: 2500,
				uniqueVehicles: 10,
				todayUploads: 5,
				loading: false,
			},
		});

		expect(screen.getByText('2.5k')).toBeTruthy();
	});
});
