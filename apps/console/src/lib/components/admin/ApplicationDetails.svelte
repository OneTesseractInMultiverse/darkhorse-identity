<script lang="ts">
	import { resolve } from '$app/paths';
	import type { Item } from '$lib/admin/catalog';
	let { application }: { application: Item } = $props();
	const sections = [
		{
			path: 'clients',
			title: 'Clients',
			tag: 'START HERE',
			description:
				'Register an OIDC client to get a client ID and secret. Configure callbacks and token settings for your application.',
			icon: '↗'
		},
		{
			path: 'resources',
			title: 'Resources',
			tag: 'PROTECTED APIs',
			description:
				'Define the APIs this application protects. Each resource has its own token audience and exposed capabilities.',
			icon: '◇'
		},
		{
			path: 'scopes',
			title: 'Scopes',
			tag: 'REQUEST LIMITS',
			description:
				'Define the permissions a client may request for a resource. Scopes limit access to permissions the user already has.',
			icon: '⌘'
		},
		{
			path: 'roles',
			title: 'Roles',
			tag: 'USER ACCESS',
			description:
				'Group capabilities into roles. Bind roles to applications, then assign them to people in the user directory.',
			icon: '☷'
		},
		{
			path: 'capabilities',
			title: 'Capabilities',
			tag: 'PERMISSIONS',
			description:
				'Define individual actions, such as invoices.read. Bind them to this application and use them in roles, resources and scopes.',
			icon: '＋'
		}
	] as const;
</script>

<div class="catalog-detail">
	<dl class="catalog-facts">
		<div class="catalog-fact-wide">
			<dt>Application ID</dt>
			<dd class="catalog-id">{application.id}</dd>
			<dd class="catalog-help">
				Identifies this application in Darkhorse. Use a client ID from Clients when configuring OIDC
				sign-in.
			</dd>
		</div>
		<div>
			<dt>Owner</dt>
			<dd>{application.owner_email}</dd>
			<dd class="catalog-help">
				The accountable contact. Ownership does not grant administrator access.
			</dd>
		</div>
		<div>
			<dt>Status</dt>
			<dd>
				<span class="catalog-status" class:inactive={!application.active}
					>{application.active ? 'Active' : 'Inactive'}</span
				>
			</dd>
			<dd class="catalog-help">
				{application.active
					? 'Registered clients can authenticate when their own settings also permit it.'
					: 'Clients in this application cannot authenticate while it is inactive.'}
			</dd>
		</div>
	</dl>
	<div class="catalog-section-heading">
		<h3>Configure this application</h3>
		<p class="catalog-help">
			Start with a client for sign-in. Add resources and permissions when the application also needs
			protected API access.
		</p>
	</div>
	<nav class="catalog-cards" aria-label="Application catalogs">
		{#each sections as section (section.path)}
			<a
				class="catalog-card"
				class:catalog-card-primary={section.path === 'clients'}
				href={resolve(
					`/console/${section.path}?application_id=${encodeURIComponent(application.id)}`
				)}
				aria-labelledby={`catalog-card-${section.path}`}
				aria-describedby={`catalog-card-${section.path}-help`}
			>
				<span class="catalog-card-icon" aria-hidden="true">{section.icon}</span>
				<div>
					<span class="catalog-kicker">{section.tag}</span>
					<h4 id={`catalog-card-${section.path}`}>
						{section.title} <span aria-hidden="true">↗</span>
					</h4>
					<p id={`catalog-card-${section.path}-help`}>{section.description}</p>
				</div>
			</a>
		{/each}
	</nav>
	<aside class="catalog-note">
		<strong>Connecting your application</strong>
		<p>
			Create a client, save its one-time secret in your application’s backend, and configure your
			OIDC library with the client ID, issuer and exact callback URL. The issuer must have OIDC
			enabled by your server operator.
		</p>
	</aside>
</div>
