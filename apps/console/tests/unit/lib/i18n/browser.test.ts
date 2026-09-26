import { expect, it, vi } from 'vitest';
import { readPreference, writePreference } from '../../../../src/lib/i18n/browser';
it('stores only a supported language independently of credentials and tolerates denied storage', () => {
	const getItem = vi.fn().mockReturnValue('es');
	const setItem = vi.fn();
	expect(readPreference({ getItem })).toBe('es');
	expect(getItem).toHaveBeenCalledWith('darkhorse.locale.v1');
	expect(writePreference({ setItem }, 'en')).toBe(true);
	expect(setItem).toHaveBeenCalledWith('darkhorse.locale.v1', 'en');
	getItem.mockReturnValue('../../es');
	expect(readPreference({ getItem })).toBeUndefined();
	getItem.mockImplementation(() => {
		throw new Error('Denied');
	});
	setItem.mockImplementation(() => {
		throw new Error('Denied');
	});
	expect(readPreference({ getItem })).toBeUndefined();
	expect(writePreference({ setItem }, 'es')).toBe(false);
});
it('handles denied access to the storage property itself through the browser adapter', async () => {
	const { browserStorage } = await import('../../../../src/lib/i18n/browser');
	const getter = vi.spyOn(window, 'localStorage', 'get').mockImplementation(() => {
		throw new DOMException('Denied', 'SecurityError');
	});
	try {
		expect(readPreference(browserStorage())).toBeUndefined();
		expect(writePreference(browserStorage(), 'en')).toBe(false);
	} finally {
		getter.mockRestore();
	}
});
