<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	let { children, active = 'users' }: { children: Snippet; active?: string } = $props();
</script>

<div class="console-shell">
	<a href="#directory-content" class="skip-link">Skip to console</a>
	<header class="console-topbar">
		<a href={resolve('/')} aria-label="Darkhorse home"
			><img src={logo} alt="Darkhorse" width="224" height="43" /></a
		><span class="console-edition">IDENTITY CONSOLE</span><a class="admin-link" href={resolve('/')}
			>My account <span aria-hidden="true">↗</span></a
		>
	</header>
	<aside class="console-sidebar">
		<p class="eyebrow">WORKSPACE</p>
		<nav aria-label="Management">
			<a href={resolve('/console/users')} aria-current={active === 'users' ? 'page' : undefined}
				><span aria-hidden="true">01</span> User directory</a
			>
			<a
				href={resolve('/console/applications')}
				aria-current={active === 'applications' ? 'page' : undefined}
				><span aria-hidden="true">02</span> Applications</a
			>
			<a href={resolve('/console/clients')} aria-current={active === 'clients' ? 'page' : undefined}
				><span aria-hidden="true">03</span> Clients</a
			>
			<a
				href={resolve('/console/resources')}
				aria-current={active === 'resources' ? 'page' : undefined}
				><span aria-hidden="true">04</span> Resources</a
			>
			<a href={resolve('/console/scopes')} aria-current={active === 'scopes' ? 'page' : undefined}
				><span aria-hidden="true">05</span> Scopes</a
			>
			<a href={resolve('/console/roles')} aria-current={active === 'roles' ? 'page' : undefined}
				><span aria-hidden="true">06</span> Roles</a
			>
			<a
				href={resolve('/console/capabilities')}
				aria-current={active === 'capabilities' ? 'page' : undefined}
				><span aria-hidden="true">07</span> Capabilities</a
			>
		</nav>
		<p class="eyebrow sidebar-section">MY SECURITY</p>
		<nav aria-label="Account security">
			<a href={resolve('/security/keys')}>Personal API keys</a>
			<a href={resolve('/security/sessions')}>Sessions</a><a href={resolve('/security/email')}
				>Email verification</a
			>
		</nav>
		<div class="sidebar-signature">DARKHORSE<br /><span>ONE ORGANIZATION. ONE IDENTITY.</span></div>
	</aside>
	<main id="directory-content" class="console-content" tabindex="-1">{@render children()}</main>
</div>
