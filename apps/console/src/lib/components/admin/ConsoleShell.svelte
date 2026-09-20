<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	let { children }: { children: Snippet } = $props();
</script>

<div class="console-shell">
	<a href="#directory-content" class="skip-link">Skip to directory</a>
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
			<a href={resolve('/console/users')} aria-current="page"
				><span aria-hidden="true">01</span> User directory</a
			>
		</nav>
		<p class="eyebrow sidebar-section">MY SECURITY</p>
		<nav aria-label="Account security">
			<a href={resolve('/security/sessions')}>Sessions</a><a href={resolve('/security/email')}
				>Email verification</a
			>
		</nav>
		<div class="sidebar-signature">DARKHORSE<br /><span>ONE ORGANIZATION. ONE IDENTITY.</span></div>
	</aside>
	<main id="directory-content" class="console-content" tabindex="-1">{@render children()}</main>
</div>
