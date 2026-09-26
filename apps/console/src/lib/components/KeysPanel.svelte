<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import Modal from '$lib/components/admin/Modal.svelte';
	import KeyEditor from './KeyEditor.svelte';
	import {
		type KeyApi,
		type Page,
		type Key,
		type Creation,
		type Failure
	} from '$lib/personal-keys';
	import { dateTime } from '$lib/i18n/display';
	const formatTime = $derived(dateTime($language.locale));
	let { api }: { api: KeyApi } = $props();
	let page = $state<Page | null>(null);
	let pending = $state(true);
	let blocked = $state(false);
	let signedOut = $state(false);
	let reauthenticate = $state(false);
	let message = $state<
		`keys.failure.${Failure['kind']}` | 'keys.concealed' | 'keys.hidden' | 'keys.revoked' | null
	>(null);
	let modal = $state<'create' | 'detail' | 'revoke' | 'secret' | null>(null);
	let selected = $state<Key | null>(null);
	let secret = $state('');
	let mounted = false;
	onMount(() => {
		mounted = true;
		void load();
		return () => {
			mounted = false;
			secret = '';
		};
	});
	function concealed() {
		if (secret) {
			secret = '';
			modal = null;
			message = 'keys.concealed';
		}
	}
	function close() {
		secret = '';
		selected = null;
		modal = null;
	}
	function failure(result: Failure) {
		message = `keys.failure.${result.kind}`;
		blocked = true;
		reauthenticate = result.kind === 'reauthenticate';
		if (result.kind === 'signed-out') {
			signedOut = true;
			language.account(undefined);
			page = null;
		}
		close();
	}
	async function load(after?: string) {
		pending = true;
		message = null;
		const result = await api.list(after);
		if (!mounted) return;
		if (result.kind === 'ready') {
			page = result.value;
			blocked = false;
			signedOut = false;
			reauthenticate = false;
		} else {
			page = null;
			failure(result);
		}
		pending = false;
	}
	async function create(input: Creation) {
		pending = true;
		message = null;
		const result = await api.create(input);
		if (!mounted) return;
		if (result.kind === 'created') {
			selected = result.key;
			page = null;
			if (document.visibilityState === 'hidden') {
				close();
				message = 'keys.hidden';
				blocked = true;
			} else {
				secret = result.secret;
				modal = 'secret';
			}
		} else failure(result);
		pending = false;
	}
	async function revoke() {
		if (!selected || pending) return;
		pending = true;
		message = null;
		const result = await api.revoke(selected.id);
		if (!mounted) return;
		close();
		if (result.kind === 'revoked') {
			await load();
			message = 'keys.revoked';
		} else failure(result);
		pending = false;
	}
	function show(key: Key, kind: 'detail' | 'revoke') {
		selected = key;
		modal = kind;
		message = null;
	}
	async function acknowledge() {
		close();
		await load();
	}
</script>

<svelte:window onpagehide={concealed} /><svelte:document
	onvisibilitychange={() => {
		if (document.visibilityState === 'hidden') concealed();
	}}
/>
<section class="glass key-panel" aria-labelledby="keys-title" aria-busy={pending}>
	<div class="directory-heading">
		<div>
			<p class="eyebrow">{$language.t('sessions.security')}</p>
			<h1 id="keys-title">{$language.t('keys.heading')}</h1>
			<p>{$language.t('keys.intro')}</p>
		</div>
		{#if !signedOut}<div class="directory-actions">
				<Button variant="outline" disabled={pending} onclick={() => load()}
					>{$language.t('keys.refresh')}</Button
				><Button
					disabled={pending || blocked || !page}
					onclick={() => {
						modal = 'create';
						message = null;
					}}>{$language.t('keys.new')}</Button
				>
			</div>{/if}
	</div>
	{#if message}<p role="alert" class="directory-notice">{$language.t(message)}</p>{/if}
	{#if signedOut || reauthenticate}<a class="admin-link" href={resolve('/')}
			>{$language.t('login.submit')}</a
		>{/if}
	{#if pending && !page}<p role="status">{$language.t('common.loading')}</p>{:else if page}
		<p class="key-scroll-hint">{$language.t('keys.scroll')}</p>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users must be able to scroll the table horizontally) -->
		<div class="directory-table" role="region" aria-label={$language.t('keys.region')} tabindex="0">
			<table>
				<thead
					><tr
						><th>{$language.t('keys.columnName')}</th><th>{$language.t('keys.created')}</th><th
							>{$language.t('keys.expires')}</th
						><th>{$language.t('common.status')}</th><th>{$language.t('common.actions')}</th></tr
					></thead
				><tbody>
					{#each page.items as key (key.id)}<tr
							><td>{key.name}</td><td>{formatTime(key.created_ms)}</td><td
								>{key.expires_ms === null
									? $language.t('keys.never')
									: formatTime(key.expires_ms)}</td
							><td>{$language.t(key.active ? 'common.active' : 'common.inactive')}</td><td
								><div class="directory-actions">
									<Button
										variant="outline"
										disabled={pending}
										onclick={() => show(key, 'detail')}
										aria-label={$language.t('keys.viewName', { name: key.name })}
										>{$language.t('keys.view')}</Button
									><Button
										variant="outline"
										disabled={pending || blocked || !key.active}
										onclick={() => show(key, 'revoke')}
										aria-label={$language.t('keys.revokeName', { name: key.name })}
										>{$language.t('keys.revoke')}</Button
									>
								</div></td
							></tr
						>{/each}
					{#if page.items.length === 0}<tr><td colspan="5">{$language.t('keys.empty')}</td></tr
						>{/if}
				</tbody>
			</table>
		</div>
		{#if page.next}<Button variant="outline" disabled={pending} onclick={() => load(page!.next!)}
				>{$language.t('keys.next')}</Button
			>{/if}
	{/if}
	<p class="key-hint">
		{$language.t('keys.help')}
	</p>
</section>
{#if modal === 'create'}<Modal title={$language.t('keys.newTitle')} {pending} {close}
		><KeyEditor read={api.options} {create} cancel={close} {pending} /></Modal
	>
{:else if modal === 'secret' && selected}<Modal
		title={$language.t('keys.saveTitle')}
		{pending}
		close={() => {
			void acknowledge();
		}}
		><p>{$language.t('keys.saveHelp')}</p>
		<label for="key-secret">{$language.t('keys.secret')}</label><textarea
			id="key-secret"
			class="key-secret"
			readonly
			value={secret}
			spellcheck="false"
			autocomplete="off"
			rows="3"></textarea>
		<p>
			{selected.name} · {selected.expires_ms === null
				? $language.t('keys.never')
				: $language.t('keys.expirationValue', { time: formatTime(selected.expires_ms) })}
		</p>
		<Button onclick={acknowledge}>{$language.t('keys.saved')}</Button></Modal
	>
{:else if modal === 'revoke' && selected}<Modal
		title={$language.t('keys.revokeTitle')}
		{pending}
		{close}
		><p>
			{$language.t('keys.revokeHelp', { name: selected.name })}
		</p>
		<div class="directory-actions">
			<Button disabled={pending} onclick={revoke}>{$language.t('keys.confirmRevoke')}</Button
			><Button variant="outline" disabled={pending} onclick={close}
				>{$language.t('common.cancel')}</Button
			>
		</div></Modal
	>
{:else if modal === 'detail' && selected}<Modal title={selected.name} {close}
		><p class="key-hint">
			{$language.t('keys.originalLimits')}
		</p>
		<p>{$language.t('keys.application')} <code>{selected.application_id}</code></p>
		{#each selected.grants as grant (grant.resource_id)}<h3>
				{$language.t('keys.resource')} <code>{grant.resource_id}</code>
			</h3>
			<ul>
				{#each grant.capabilities as cap (cap)}<li><code>{cap}</code></li>{/each}
			</ul>{/each}<Button onclick={close}>{$language.t('common.close')}</Button></Modal
	>{/if}
