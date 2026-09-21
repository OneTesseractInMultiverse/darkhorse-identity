import { describe, it, expect, vi } from 'vitest';
import {
	profileApi,
	decodeProfile,
	decodeOptions,
	fields,
	message
} from '../../../src/lib/profiles';
export const profile = {
	id: '00000000-0000-0000-0000-000000000001',
	revision: '0',
	email: 'ana@example.com',
	active: true,
	email_verified: false,
	first_name: 'Ana',
	second_name: '',
	last_name: 'Guzmán',
	second_last_name: '',
	country: 'CR',
	calling_code: '506',
	national_number: '88887777',
	bio: 'Hello'
};
describe('profile transport', () => {
	it('validates bounded data and projects only editable fields', () => {
		expect(decodeProfile(profile)).toEqual(profile);
		expect(decodeProfile(null)).toBeNull();
		for (const invalid of [
			{ ...profile, revision: '01' },
			{ ...profile, id: 'bad' },
			{ ...profile, bio: 'x'.repeat(2001) },
			{ ...profile, active: 'yes' },
			{ ...profile, country: 'bad' }
		])
			expect(decodeProfile(invalid)).toBeNull();
		expect(fields(profile)).not.toHaveProperty('email');
		expect(fields(profile)).not.toHaveProperty('active');
		expect(
			decodeOptions({
				country_version: 'test',
				phone_version: 'test',
				countries: [{ code: 'CR', name: 'Costa Rica' }],
				calling_codes: ['506']
			})
		).not.toBeNull();
		for (const options of [
			null,
			{},
			{
				country_version: 'test',
				phone_version: 'test',
				countries: [{ code: 'ZZZ', name: 'bad' }],
				calling_codes: []
			},
			{ country_version: 'test', phone_version: 'test', countries: [], calling_codes: ['+1'] }
		])
			expect(decodeOptions(options)).toBeNull();
	});
	it('never retries uncertain writes and uses same-origin CSRF', async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(new Response(JSON.stringify(profile)))
			.mockRejectedValueOnce(new Error('untrusted'));
		const api = profileApi(fetcher);
		expect(await api.load()).toEqual({ kind: 'ready', value: profile });
		expect(await api.save('me', '0', fields(profile))).toEqual({ kind: 'uncertain' });
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(fetcher.mock.calls[1][1]).toMatchObject({
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'x-darkhorse-csrf': '1' }
		});
		expect(JSON.parse(fetcher.mock.calls[1][1].body)).not.toHaveProperty('email');
	});
	it('maps failure states without displaying server messages', async () => {
		for (const [status, kind] of [
			[401, 'signed-out'],
			[403, 'denied'],
			[409, 'changed'],
			[400, 'invalid'],
			[503, 'uncertain']
		] as const) {
			const fetcher = vi.fn().mockResolvedValue(new Response('{}', { status }));
			expect((await profileApi(fetcher).save('me', '0', fields(profile))).kind).toBe(kind);
		}
		const recent = vi
			.fn()
			.mockResolvedValue(
				new Response('{"error":"recent_authentication_required"}', { status: 403 })
			);
		expect((await profileApi(recent).load()).kind).toBe('recent');
		const malformed = vi.fn().mockResolvedValue(new Response('broken'));
		expect((await profileApi(malformed).load()).kind).toBe('unavailable');
		for (const kind of [
			'signed-out',
			'denied',
			'changed',
			'invalid',
			'recent',
			'uncertain',
			'unavailable'
		] as const)
			expect(message({ kind })).toBeTruthy();
	});
});

it('loads canonical options, saves confirmed edits, and rejects malformed responses', async () => {
	const options = {
		country_version: 'test',
		phone_version: 'test',
		countries: [{ code: 'CR', name: 'Costa Rica' }],
		calling_codes: ['506']
	};
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(new Response(JSON.stringify(options)))
		.mockResolvedValueOnce(new Response(JSON.stringify(profile)))
		.mockResolvedValueOnce(new Response('{}'))
		.mockResolvedValueOnce(new Response('invalid json', { status: 403 }));
	const api = profileApi(fetcher);
	expect(await api.options()).toEqual({ kind: 'ready', value: options });
	expect((await api.save('me', '0', fields(profile))).kind).toBe('ready');
	expect((await api.save('me', '0', fields(profile))).kind).toBe('uncertain');
	expect((await api.load()).kind).toBe('denied');
	expect(decodeOptions({ ...options, calling_codes: ['506', '506'] })).toBeNull();
	expect(
		decodeOptions({ ...options, countries: [...options.countries, ...options.countries] })
	).toBeNull();
});
