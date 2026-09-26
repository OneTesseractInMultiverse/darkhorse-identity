import { decodePolicyMap, type PolicyMap } from './policy-map';

export type PolicyMapRead =
	| { kind: 'ready'; data: PolicyMap }
	| { kind: 'signed-out' | 'forbidden' | 'too-large' | 'unavailable' | 'aborted' };

/** Read one no-store, same-origin application snapshot through the administrator session. */
export async function readPolicyMap(
	fetcher: typeof fetch,
	applicationId: string,
	signal?: AbortSignal
): Promise<PolicyMapRead> {
	try {
		const response = await fetcher(
			`/api/admin/console/applications/${encodeURIComponent(applicationId)}/policy-map`,
			{ method: 'GET', credentials: 'same-origin', cache: 'no-store', signal }
		);
		if (response.status === 401) return { kind: 'signed-out' };
		if (response.status === 403) return { kind: 'forbidden' };
		if (response.status === 413) return { kind: 'too-large' };
		if (!response.ok) return { kind: 'unavailable' };
		const data = decodePolicyMap(await response.json());
		return data ? { kind: 'ready', data } : { kind: 'unavailable' };
	} catch (error) {
		return signal?.aborted || (error instanceof DOMException && error.name === 'AbortError')
			? { kind: 'aborted' }
			: { kind: 'unavailable' };
	}
}
