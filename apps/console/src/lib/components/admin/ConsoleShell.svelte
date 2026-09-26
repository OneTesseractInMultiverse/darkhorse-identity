<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	let { children, active = 'users' }: { children: Snippet; active?: string } = $props();
</script>

<div class="console-shell">
	<a href="#directory-content" class="skip-link" lang={$language.locale}
		>{$language.t('console.skip')}</a
	>
	<header class="console-topbar" lang={$language.locale}>
		<a href={resolve('/')} aria-label={$language.t('portal.home')}
			><img src={logo} alt="Darkhorse" width="224" height="43" /></a
		><span class="console-edition">{$language.t('console.edition')}</span><a
			class="admin-link"
			href={resolve('/')}>{$language.t('console.account')} <span aria-hidden="true">↗</span></a
		>
		<LanguageSelector />
	</header>
	<aside class="console-sidebar" lang={$language.locale}>
		<p class="eyebrow">{$language.t('console.workspace')}</p>
		<nav aria-label={$language.t('console.management')}>
			<a href={resolve('/console/users')} aria-current={active === 'users' ? 'page' : undefined}
				><span aria-hidden="true">01</span> {$language.t('console.users')}</a
			>
			<a
				href={resolve('/console/applications')}
				aria-current={active === 'applications' ? 'page' : undefined}
				><span aria-hidden="true">02</span> {$language.t('console.applications')}</a
			>
			<a href={resolve('/console/clients')} aria-current={active === 'clients' ? 'page' : undefined}
				><span aria-hidden="true">03</span> {$language.t('console.clients')}</a
			>
			<a
				href={resolve('/console/resources')}
				aria-current={active === 'resources' ? 'page' : undefined}
				><span aria-hidden="true">04</span> {$language.t('console.resources')}</a
			>
			<a href={resolve('/console/scopes')} aria-current={active === 'scopes' ? 'page' : undefined}
				><span aria-hidden="true">05</span> {$language.t('console.scopes')}</a
			>
			<a href={resolve('/console/roles')} aria-current={active === 'roles' ? 'page' : undefined}
				><span aria-hidden="true">06</span> {$language.t('console.roles')}</a
			>
			<a
				href={resolve('/console/capabilities')}
				aria-current={active === 'capabilities' ? 'page' : undefined}
				><span aria-hidden="true">07</span> {$language.t('console.capabilities')}</a
			>
			<a
				href={resolve('/console/settings')}
				aria-current={active === 'settings' ? 'page' : undefined}
				><span aria-hidden="true">08</span> {$language.t('console.settings')}</a
			>
		</nav>
		<p class="eyebrow sidebar-section">{$language.t('console.security')}</p>
		<nav aria-label={$language.t('console.securityNav')}>
			<a href={resolve('/account/profile')}>{$language.t('console.profile')}</a>
			<a href={resolve('/security/keys')}>{$language.t('console.keys')}</a>
			<a href={resolve('/security/sessions')}>{$language.t('console.sessions')}</a><a
				href={resolve('/security/email')}>{$language.t('console.email')}</a
			>
		</nav>
		<div class="sidebar-signature">
			DARKHORSE<br /><span>{$language.t('console.signature')}</span>
		</div>
	</aside>
	<main id="directory-content" class="console-content" tabindex="-1">{@render children()}</main>
</div>

<style>
	.console-topbar {
		flex-wrap: wrap;
	}
</style>
