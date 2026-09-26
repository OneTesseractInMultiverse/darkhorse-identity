<script lang="ts">
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	let {
		name,
		pending,
		error,
		signOut
	}: {
		name: string;
		pending: boolean;
		error: string;
		signOut?: () => Promise<void>;
	} = $props();
	const actions = [
		{
			label: 'My profile',
			description: 'Your name, contact details and profile picture.',
			href: resolve('/account/profile'),
			icon: 'M20 21v-2a6 6 0 0 0-6-6h-4a6 6 0 0 0-6 6v2M16 6a4 4 0 1 1-8 0 4 4 0 0 1 8 0'
		},
		{
			label: 'Manage sessions',
			description: 'Review your sign-ins and end sessions you no longer use.',
			href: resolve('/security/sessions'),
			icon: 'M3 3h18v13H3zM8 21h8M12 16v5M7 7h10M7 11h6'
		},
		{
			label: 'Manage API keys',
			description: 'Give scripts and services a limited portion of your access.',
			href: resolve('/security/keys'),
			icon: 'M15 3a6 6 0 0 0-5.5 8.4L3 18v3h4v-3h3v-3l2.6-2.5A6 6 0 1 0 15 3ZM16 7h.01'
		},
		{
			label: 'Verify email',
			description: 'Confirm your email address and check its verification status.',
			href: resolve('/security/email'),
			icon: 'M3 5h18v14H3zM3 5l9 7 9-7'
		}
	];
</script>

<svelte:head><title>Your account — Darkhorse</title></svelte:head>

<section class="glass account-overview" aria-labelledby="account-title" aria-busy={pending}>
	<div class="account-toolbar">
		<p class="eyebrow">ACCOUNT / OVERVIEW</p>
		<div class="account-controls">
			<span class="session-status"><span class="dot"></span>Signed in</span>
			{#if signOut}<Button variant="outline" class="h-10 px-4" onclick={signOut} disabled={pending}
					>{pending ? 'Signing out…' : 'Sign out'}</Button
				>{/if}
		</div>
	</div>
	<div class="account-body">
		<div class="account-greeting">
			<h1 id="account-title">Welcome, {name}.</h1>
			<p>Your identity, access and security. All in one place.</p>
		</div>
		{#if error}<p role="alert" class="account-error">{error}</p>{/if}
		<section class="console-entry" aria-labelledby="console-entry-title">
			<div class="console-symbol" aria-hidden="true">
				<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
					><rect x="2" y="3" width="20" height="18" rx="3" /><path d="m6 8 4 4-4 4m7 0h5" /></svg
				>
			</div>
			<div class="console-copy">
				<p class="eyebrow">ORGANIZATION</p>
				<h2 id="console-entry-title">Management console</h2>
				<p>Manage people, applications and access across your organization.</p>
				<span class="console-requirement">For platform administrators</span>
			</div>
			<Button href={resolve('/console/users')} class="h-12 gap-6 px-6"
				>Open console <span aria-hidden="true">↗</span></Button
			>
		</section>
		<div class="tools-heading">
			<h2>Your account</h2>
			<span class="eyebrow">PROFILE & SECURITY</span>
		</div>
		<nav class="account-tools" aria-label="Account tools">
			{#each actions as action (action.href)}
				<a class="account-action" href={action.href} aria-label={action.label}>
					<div class="action-top">
						<svg
							aria-hidden="true"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="1.5"
							stroke-linecap="round"
							stroke-linejoin="round"><path d={action.icon} /></svg
						>
						<span class="action-arrow" aria-hidden="true">↗</span>
					</div>
					<h3>{action.label}</h3>
					<p>{action.description}</p>
				</a>
			{/each}
		</nav>
	</div>
</section>

<style>
	.account-overview {
		width: min(100%, 1120px);
		min-width: 0;
	}
	.account-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 20px 36px;
		border-bottom: 1px solid var(--console-line);
	}
	.account-controls {
		display: flex;
		align-items: center;
		gap: 24px;
	}
	.session-status {
		color: var(--color-primary);
		font-size: 12px;
		white-space: nowrap;
	}
	.account-body {
		padding: 36px;
	}
	.account-greeting h1 {
		font-size: clamp(32px, 4vw, 48px);
		line-height: 1.15;
		letter-spacing: -0.045em;
		margin: 0 0 12px;
		overflow-wrap: anywhere;
	}
	.account-greeting > p {
		color: var(--color-muted-foreground);
		line-height: 1.6;
	}
	.console-entry {
		display: flex;
		align-items: center;
		gap: 24px;
		padding: 28px;
		margin-top: 32px;
		border: 1px solid #91e4b43d;
		border-radius: 16px;
		background: linear-gradient(110deg, #91e4b410, #91e4b404);
	}
	.console-symbol {
		color: var(--color-primary);
		background: #91e4b410;
		border: 1px solid #91e4b426;
		border-radius: 14px;
		padding: 16px;
		flex-shrink: 0;
	}
	.console-symbol svg {
		width: 32px;
		height: 32px;
	}
	.console-copy {
		flex: 1;
		min-width: 0;
	}
	.console-copy h2 {
		font-size: 24px;
		font-weight: 500;
		letter-spacing: -0.03em;
		margin: 6px 0;
	}
	.console-copy > p:not(.eyebrow) {
		font-size: 14px;
		line-height: 1.6;
		color: #b1c0b7;
	}
	.console-requirement {
		display: block;
		font-size: 12px;
		color: var(--color-muted-foreground);
		margin-top: 12px;
	}
	.tools-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		margin: 32px 0 16px;
	}
	.tools-heading h2 {
		font-size: 18px;
		font-weight: 500;
	}
	.account-tools {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 14px;
	}
	.account-action {
		display: block;
		padding: 22px;
		border: 1px solid var(--console-line);
		border-radius: 14px;
		background: #0a130e66;
		text-decoration: none;
		transition:
			border-color 160ms,
			background-color 160ms;
	}
	.account-action:hover,
	.account-action:focus-visible {
		border-color: #91e4b47a;
		background: #91e4b40a;
	}
	.action-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 24px;
		color: var(--color-primary);
	}
	.action-top svg {
		width: 24px;
		height: 24px;
	}
	.action-arrow {
		color: var(--color-muted-foreground);
	}
	.account-action h3 {
		font-size: 15px;
		font-weight: 500;
		margin-bottom: 8px;
	}
	.account-action p {
		color: var(--color-muted-foreground);
		font-size: 13px;
		line-height: 1.65;
	}
	.account-error {
		color: #ffb4b4;
		padding: 16px;
		border: 1px solid #ffb4b440;
		border-radius: 10px;
		margin-top: 24px;
	}
	@media (max-width: 900px) {
		.account-tools {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.console-symbol {
			display: none;
		}
	}
	@media (max-width: 600px) {
		.account-toolbar {
			padding: 18px 22px;
			flex-wrap: wrap;
		}
		.account-controls {
			gap: 16px;
		}
		.account-body {
			padding: 24px 22px;
		}
		.console-entry {
			align-items: stretch;
			flex-direction: column;
			padding: 22px;
		}
		.account-tools {
			grid-template-columns: minmax(0, 1fr);
		}
		.tools-heading {
			flex-wrap: wrap;
			gap: 8px;
		}
		.account-action {
			padding: 20px;
		}
		.action-top {
			margin-bottom: 16px;
		}
	}
</style>
