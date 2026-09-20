import type { Command, Item } from './catalog';
export function binding(
	target: Item,
	related: string,
	applicationBinding: boolean,
	adding: boolean
): Command {
	if (applicationBinding)
		return target.kind === 'role'
			? { operation: 'role_binding', application_id: related, role_id: target.id, bound: adding }
			: {
					operation: 'capability_binding',
					application_id: related,
					capability_id: target.id,
					bound: adding
				};
	if (target.kind === 'role')
		return {
			operation: 'role_capability',
			role_id: target.id,
			capability_id: related,
			granted: adding
		};
	if (target.kind === 'resource')
		return {
			operation: 'resource_capability',
			application_id: target.application_id,
			resource_id: target.id,
			capability_id: related,
			exposed: adding
		};
	return {
		operation: 'scope_capability',
		application_id: target.application_id,
		resource_id: target.resource_id,
		scope_id: target.id,
		capability_id: related,
		included: adding
	};
}
