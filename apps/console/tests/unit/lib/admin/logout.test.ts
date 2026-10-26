import { describe, expect, it, vi } from 'vitest';
import { configuration, record, logoutApi } from '#lib/admin/logout.ts';
const config = {
	revision: '9223372036854775807',
	backchannel_logout_uri: 'https://client.example/logout',
	backchannel_logout_session_required: true
};
describe('logout configuration', () => {
	it('preserves exact revisions and rejects unsupported or malformed data', () => {
		expect(configuration(config)).toEqual(config);
		for (const revision of ['00', '-1', '9223372036854775808', 1])
			expect(configuration({ ...config, revision })).toBeNull();
		expect(configuration({ ...config, backchannel_logout_session_required: false })).toBeNull();
		expect(
			record({
				configuration: config,
				deliveries: [{ id: 'bad', state: 'delivered', attempts: 1, created_ms: 1 }]
			})
		).toBeNull();
		expect(record({ configuration: config, deliveries: [] })).toEqual({
			configuration: config,
			deliveries: []
		});
	});
	it('writes through the protected same-origin endpoint and does not retry uncertain writes', async () => {
		const fetcher = vi.fn<typeof fetch>().mockRejectedValue(new Error('lost response'));
		expect(
			await logoutApi(fetcher, 'app', 'client').save({
				...config,
				backchannel_logout_session_required: true
			})
		).toEqual({ kind: 'uncertain' });
		expect(fetcher).toHaveBeenCalledTimes(1);
		expect(fetcher).toHaveBeenCalledWith(
			'/api/admin/applications/app/clients/client/logout',
			expect.objectContaining({
				method: 'PUT',
				credentials: 'same-origin',
				cache: 'no-store',
				headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' }
			})
		);
	});
});
