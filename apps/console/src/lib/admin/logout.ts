import { read, write } from './catalog';
export type Configuration = {
	revision: string;
	backchannel_logout_uri: string | null;
	backchannel_logout_session_required: true;
};
export type Delivery = {
	id: string;
	state: 'queued' | 'delivered' | 'failed' | 'cancelled';
	attempts: number;
	created_ms: number;
};
export type Logout = { configuration: Configuration; deliveries: Delivery[] };
function object(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}
export function configuration(value: unknown): Configuration | null {
	if (
		!object(value) ||
		typeof value.revision !== 'string' ||
		!/^(0|[1-9][0-9]*)$/.test(value.revision) ||
		value.revision.length > 19 ||
		BigInt(value.revision) > 9223372036854775807n ||
		value.backchannel_logout_session_required !== true ||
		!(
			value.backchannel_logout_uri === null ||
			(typeof value.backchannel_logout_uri === 'string' &&
				value.backchannel_logout_uri.length <= 2048 &&
				value.backchannel_logout_uri.startsWith('https://'))
		)
	)
		return null;
	return value as Configuration;
}
export function record(value: unknown): Logout | null {
	if (
		!object(value) ||
		!configuration(value.configuration) ||
		!Array.isArray(value.deliveries) ||
		value.deliveries.length > 20 ||
		!value.deliveries.every(delivery)
	)
		return null;
	return value as Logout;
}
function delivery(value: unknown): value is Delivery {
	return (
		object(value) &&
		typeof value.id === 'string' &&
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value.id) &&
		['queued', 'delivered', 'failed', 'cancelled'].includes(String(value.state)) &&
		Number.isInteger(value.attempts) &&
		Number(value.attempts) >= 0 &&
		Number(value.attempts) <= 8 &&
		Number.isSafeInteger(value.created_ms) &&
		Number(value.created_ms) >= 0
	);
}
export function logoutApi(fetcher: typeof fetch, application: string, client: string) {
	const path = `/api/admin/applications/${encodeURIComponent(application)}/clients/${encodeURIComponent(client)}/logout`;
	return {
		read: () => read(fetcher, path, record),
		save: (value: Configuration) => write(fetcher, path, value, configuration, 'PUT')
	};
}
