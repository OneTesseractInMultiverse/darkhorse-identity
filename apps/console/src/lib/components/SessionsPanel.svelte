<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import { dateTime } from '$lib/i18n/display';
	const language = useLocalization();
	const formatTime = $derived(dateTime($language.locale));
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import { type SessionRecord, type SessionsState, type Termination } from '$lib/sessions';
	let {
		read,
		end
	}: {
		read: (after?: string) => Promise<SessionsState>;
		end: (id: string) => Promise<Termination>;
	} = $props();
	let sessionState = $state<SessionsState>({ kind: 'unavailable' });
	let pending = $state(true);
	let uncertain = $state(false);
	let older = $state(false);
	let selected = $state<SessionRecord | null>(null);
	let dialog = $state<HTMLDialogElement>();
	let message = $state<'sessions.unavailable' | 'sessions.uncertain' | null>(null);
	let alert = $state<HTMLParagraphElement>();
	let alive = false;
	onMount(() => {
		alive = true;
		void load();
		return () => {
			alive = false;
		};
	});
	$effect(() => {
		if (selected && dialog && !dialog.open) dialog.showModal();
	});
	async function load(after?: string) {
		pending = true;
		message = null;
		const result = await read(after);
		if (!alive) return;
		sessionState = result;
		older = after !== undefined;
		if (sessionState.kind === 'signed-out') language.account(undefined);
		if (sessionState.kind === 'ready') uncertain = false;
		else if (sessionState.kind === 'unavailable') message = 'sessions.unavailable';
		pending = false;
	}
	function cancel() {
		dialog?.close();
		selected = null;
	}
	async function confirm() {
		if (!selected || pending) return;
		pending = true;
		const result = await end(selected.id);
		if (!alive) return;
		cancel();
		if (result.kind === 'signed-out' || (result.kind === 'ended' && result.current)) {
			sessionState = { kind: 'signed-out' };
			language.account(undefined);
			pending = false;
			return;
		}
		if (result.kind === 'ended') {
			await load();
			return;
		}
		uncertain = true;
		pending = false;
		message = 'sessions.uncertain';
		await tick();
		alert?.focus();
	}
</script>

<section class="glass session-panel" aria-labelledby="sessions-title" aria-busy={pending}>
	<div class="panel-top">
		<span><span class="dot"></span> {$language.t('sessions.security')}</span><span
			>{$language.t('sessions.edition')}</span
		>
	</div>
	<div class="session-body">
		<div class="session-heading">
			<div>
				<h1 id="sessions-title">{$language.t('sessions.heading')}</h1>
				<p>{$language.t('sessions.intro')}</p>
			</div>
			{#if sessionState.kind !== 'signed-out'}<Button
					variant="outline"
					onclick={() => load()}
					disabled={pending}>{$language.t('sessions.refresh')}</Button
				>{/if}
		</div>
		{#if pending && sessionState.kind !== 'ready'}<p role="status">
				{$language.t('sessions.loading')}
			</p>
		{:else if sessionState.kind === 'signed-out'}
			<p>{$language.t('sessions.signedOut')}</p>
			<a class="security-link" href={resolve('/')}>{$language.t('login.submit')}</a>
		{:else if sessionState.kind === 'ready'}
			<div class="table-wrap">
				<table>
					<caption>{$language.t('sessions.caption')}</caption>
					<thead
						><tr
							><th scope="col">{$language.t('sessions.started')}</th><th scope="col"
								>{$language.t('sessions.lastActivity')}</th
							><th scope="col">{$language.t('common.status')}</th><th scope="col"
								>{$language.t('common.actions')}</th
							></tr
						></thead
					>
					<tbody
						>{#each sessionState.page.items as session (session.id)}<tr>
								<th scope="row"
									><time datetime={new Date(session.created_ms).toISOString()}
										>{formatTime(session.created_ms)}</time
									>{#if session.id === sessionState.page.current}<span class="current"
											>{$language.t('sessions.current')}</span
										>{/if}</th
								>
								<td
									><time datetime={new Date(session.seen_ms).toISOString()}
										>{formatTime(session.seen_ms)}</time
									></td
								>
								<td
									><span class:active={session.status === 'active'} class="state"
										>{$language.t(
											session.status === 'active' ? 'common.active' : 'sessions.inactive'
										)}</span
									></td
								>
								<td
									>{#if session.status === 'active'}<Button
											variant="outline"
											disabled={pending || uncertain}
											onclick={() => {
												selected = session;
											}}
											>{session.id === sessionState.page.current
												? $language.t('sessions.endCurrent')
												: $language.t('sessions.end')}</Button
										>{:else}<span aria-label={$language.t('sessions.noAction')}>—</span>{/if}</td
								>
							</tr>{/each}</tbody
					>
				</table>
			</div>
			{#if sessionState.page.items.length === 0}<p class="empty">
					{$language.t('sessions.empty')}
				</p>{/if}
			<nav class="pagination" aria-label={$language.t('sessions.pages')}>
				{#if older}<Button variant="outline" disabled={pending} onclick={() => load()}
						>{$language.t('sessions.newest')}</Button
					>{/if}
				{#if sessionState.page.next}<Button
						variant="outline"
						disabled={pending}
						onclick={() => {
							if (sessionState.kind === 'ready' && sessionState.page.next)
								void load(sessionState.page.next);
						}}>{$language.t('sessions.older')}</Button
					>{/if}
			</nav>
			<p class="session-help">
				{$language.t('sessions.help')}
			</p>
		{/if}
		{#if message}<p role="alert" tabindex="-1" bind:this={alert} class="login-error">
				{$language.t(message)}
			</p>{/if}
	</div>
</section>
{#if selected}
	<dialog
		bind:this={dialog}
		onclose={() => {
			selected = null;
		}}
		oncancel={(event) => {
			if (pending) event.preventDefault();
		}}
		aria-labelledby="end-session-title"
		aria-describedby="end-session-detail"
	>
		<h2 id="end-session-title">{$language.t('sessions.confirmTitle')}</h2>
		<p id="end-session-detail">
			{$language.t('sessions.confirmDetail', { time: formatTime(selected.created_ms) })}
		</p>
		{#if sessionState.kind === 'ready' && selected.id === sessionState.page.current}<p>
				{$language.t('sessions.confirmCurrent')}
			</p>{/if}
		<div class="dialog-actions">
			<Button variant="outline" onclick={cancel} disabled={pending}
				>{$language.t('common.cancel')}</Button
			><Button onclick={confirm} disabled={pending}
				>{$language.t(pending ? 'sessions.ending' : 'sessions.confirm')}</Button
			>
		</div>
	</dialog>
{/if}

<style>
	.session-panel {
		min-width: 0;
		width: min(100%, 1120px);
	}
	.session-body {
		padding: 32px;
	}
	.session-heading {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 24px;
		margin-bottom: 24px;
	}
	h1 {
		font-size: 36px;
		margin: 0 0 14px;
		letter-spacing: -0.035em;
	}
	p {
		color: #b1c0b7;
		line-height: 1.65;
	}
	.table-wrap {
		overflow-x: auto;
		border: 1px solid #96b8a122;
		border-radius: 12px;
	}
	table {
		width: 100%;
		min-width: 740px;
		border-collapse: collapse;
		text-align: left;
	}
	caption {
		text-align: left;
		padding: 16px 20px;
		font:
			11px ui-monospace,
			monospace;
		color: #9aaea2;
	}
	th,
	td {
		padding: 18px 20px;
		border-top: 1px solid #96b8a122;
		font-size: 13px;
	}
	thead th {
		text-transform: uppercase;
		font:
			10px ui-monospace,
			monospace;
		letter-spacing: 0.1em;
		color: #9aaea2;
		background: #07110a50;
	}
	tbody th {
		font-weight: 400;
	}
	time {
		white-space: nowrap;
		font-family: ui-monospace, monospace;
		font-size: 12px;
	}
	.current {
		display: block;
		margin-top: 8px;
		color: #91e4b4;
		font-size: 11px;
	}
	.state {
		color: #9aaea2;
		white-space: nowrap;
		font-size: 12px;
	}
	.state.active {
		color: #91e4b4;
	}
	.pagination {
		display: flex;
		justify-content: flex-end;
		gap: 12px;
		margin-top: 20px;
	}
	.session-help {
		font-size: 12px;
		max-width: 780px;
		margin-top: 24px;
	}
	.empty {
		padding: 20px;
	}
	.security-link {
		display: inline-block;
		color: #91e4b4;
		margin-top: 12px;
	}
	dialog {
		max-width: min(520px, calc(100vw - 40px));
		margin: auto;
		padding: 30px;
		border-radius: 20px;
		border: 1px solid #96b8a150;
		background: #152019;
		color: #eaf0ed;
		box-shadow: 0 24px 80px #0009;
	}
	dialog::backdrop {
		background: #0009;
		backdrop-filter: blur(6px);
	}
	dialog h2 {
		font-size: 24px;
		margin: 0 0 16px;
	}
	.dialog-actions {
		display: flex;
		justify-content: flex-end;
		gap: 12px;
		margin-top: 28px;
	}
	@media (max-width: 600px) {
		.session-body {
			padding: 20px;
		}
		.session-heading {
			align-items: flex-start;
			flex-direction: column;
		}
		h1 {
			font-size: 30px;
		}
		.dialog-actions {
			flex-wrap: wrap;
		}
	}
</style>
