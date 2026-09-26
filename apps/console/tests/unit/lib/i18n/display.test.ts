import { expect, it } from 'vitest';
import { countries, dateTime } from '../../../../src/lib/i18n/display';
it('localizes only supplied country names and ordering while preserving codes and source data', () => {
	const source = [
		{ code: 'US', name: 'United States' },
		{ code: 'DE', name: 'Germany' },
		{ code: 'CR', name: 'Costa Rica' }
	];
	expect(countries('es', source)).toEqual([
		{ code: 'DE', name: 'Alemania' },
		{ code: 'CR', name: 'Costa Rica' },
		{ code: 'US', name: 'Estados Unidos' }
	]);
	expect(source[0]).toEqual({ code: 'US', name: 'United States' });
	expect(countries('en', source).map((c) => c.code)).toEqual(['CR', 'DE', 'US']);
});
it('formats dates in the selected language with explicit UTC and leaves the instant unchanged', () => {
	const value = Date.UTC(2026, 8, 26, 3, 4, 5);
	expect(dateTime('es')(value)).toContain('UTC');
	expect(dateTime('es')(value)).toContain('26');
	expect(dateTime('es')(value)).toContain('03:04:05');
	expect(dateTime('en')(value)).not.toEqual(dateTime('es')(value));
	expect(value).toBe(Date.UTC(2026, 8, 26, 3, 4, 5));
});
it('uses deterministic source/UTC fallbacks when platform display services are unavailable', async () => {
	const { vi } = await import('vitest');
	const names = vi.spyOn(Intl, 'DisplayNames').mockImplementation(() => {
		throw new Error('unavailable');
	});
	const dates = vi.spyOn(Intl, 'DateTimeFormat').mockImplementation(() => {
		throw new Error('unavailable');
	});
	try {
		expect(
			countries('es', [
				{ code: 'US', name: 'United States' },
				{ code: 'CR', name: 'Costa Rica' }
			])
		).toEqual([
			{ code: 'CR', name: 'Costa Rica' },
			{ code: 'US', name: 'United States' }
		]);
		expect(dateTime('es')(1000)).toBe('1970-01-01 00:00:01 UTC');
	} finally {
		names.mockRestore();
		dates.mockRestore();
	}
});
