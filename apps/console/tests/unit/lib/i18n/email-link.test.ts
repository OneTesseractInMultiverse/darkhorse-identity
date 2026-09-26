import { expect, it } from 'vitest';
import { emailLink } from '../../../../src/lib/i18n/email-link';
it('accepts legacy and fixed language fragments without changing proof purpose', () => {
	for (const purpose of ['ev1', 'iv1'] as const) {
		const token = `${purpose}_${'a'.repeat(64)}`;
		expect(emailLink(`#token=${token}`, purpose)).toEqual({ token, locale: undefined });
		for (const locale of ['en', 'es']) {
			expect(emailLink(`#token=${token}&lang=${locale}`, purpose)).toEqual({ token, locale });
		}
		for (const fragment of [
			`#token=${token}&lang=ES`,
			`#token=${token}&lang=fr`,
			`#token=${token}&lang=es&lang=en`,
			`#lang=es&token=${token}`,
			`#token=${token}&lang=es\n`,
			`#token=${token}&next=evil`,
			`#token=${token.toUpperCase()}`,
			'#token=' + 'a'.repeat(4096)
		]) {
			expect(emailLink(fragment, purpose)).toBeUndefined();
		}
	}
	expect(emailLink(`#token=iv1_${'a'.repeat(64)}&lang=es`, 'ev1')).toBeUndefined();
});
