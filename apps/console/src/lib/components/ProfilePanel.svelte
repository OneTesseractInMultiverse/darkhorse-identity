<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		type ProfileApi,
		type Profile,
		type Options,
		type Fields,
		type Failure
	} from '$lib/profiles';
	import ImageControl from './ImageControl.svelte';
	import avatar from '$lib/assets/brand/avatar.svg';
	import type { MediaApi } from '$lib/media';
	import Modal from './admin/Modal.svelte';
	import ProfileEditor from './ProfileEditor.svelte';
	import LanguageEditor from './LanguageEditor.svelte';
	import { countries } from '$lib/i18n/display';
	import { useLocalization } from '$lib/i18n/context';
	import type { Locale } from '$lib/i18n/locale';
	const language = useLocalization();
	let languageEditing = $state(false);
	let {
		api,
		images,
		target = 'me'
	}: { api: ProfileApi; images?: MediaApi; target?: string } = $props();
	let pictureEditing = $state(false),
		pictureFailed = $state(false);
	let profile = $state<Profile | null>(null),
		options = $state<Options | null>(null),
		pending = $state(true),
		editing = $state(false),
		blocked = $state(false),
		error = $state<Failure | null>(null),
		notice = $state<'profile.saved' | 'profile.languageSaved' | null>(null);
	const countryLabel = $derived(
		profile?.country
			? countries($language.locale, [{ code: profile.country, name: profile.country }])[0].name
			: ''
	);
	let alive = true;
	function fail(e: Failure) {
		error = e;
		blocked = true;
		if (e.kind === 'signed-out' || e.kind === 'denied') {
			if (target === 'me') language.account(undefined);
			profile = null;
			options = null;
		}
	}
	async function load() {
		if (!alive) return;
		pending = true;
		editing = false;
		languageEditing = false;
		pictureEditing = false;
		pictureFailed = false;
		notice = null;
		error = null;
		const result = await api.load(target);
		if (!alive) return;
		if (result.kind === 'ready') {
			profile = result.value;
			if (target === 'me') language.account(profile.preferred_locale ?? undefined);
			blocked = false;
		} else fail(result);
		pending = false;
	}
	async function edit() {
		pending = true;
		notice = null;
		const result = await api.options();
		if (!alive) return;
		if (result.kind === 'ready') {
			options = result.value;
			editing = true;
		} else fail(result);
		pending = false;
	}
	async function save(fields: Fields) {
		if (!profile || pending || blocked) return;
		pending = true;
		error = null;
		const result = await api.save(target, profile.revision, fields);
		if (!alive) return;
		editing = false;
		if (result.kind === 'ready') {
			profile = result.value;
			if (target === 'me') language.account(profile.preferred_locale ?? undefined);
			notice = 'profile.saved';
		} else fail(result);
		pending = false;
	}
	async function saveLanguage(locale: Locale | null) {
		if (!profile || pending || blocked || target !== 'me') return;
		pending = true;
		error = null;
		notice = null;
		const result = await api.language(profile.revision, locale);
		if (!alive) return;
		languageEditing = false;
		if (result.kind === 'ready') {
			profile = result.value;
			language.account(profile.preferred_locale ?? undefined);
			notice = 'profile.languageSaved';
		} else fail(result);
		pending = false;
	}

	onMount(() => {
		alive = true;
		void load();
		return () => {
			alive = false;
			profile = null;
			options = null;
		};
	});
</script>

<section class="glass profile-panel" aria-labelledby="profile-title">
	<p class="eyebrow">{$language.t('profile.eyebrow')}</p>
	<h1 id="profile-title">{$language.t(target === 'me' ? 'profile.mine' : 'profile.user')}</h1>
	<p>{$language.t('profile.intro')}</p>
	{#if error}<p role="alert">{$language.t(`profile.error.${error.kind}`)}</p>{/if}{#if notice}<p
			role="status"
		>
			{$language.t(notice)}
		</p>{/if}
	{#if pending}<p role="status">{$language.t('common.loading')}</p>{/if}
	{#if profile}
		{#key profile.revision}<img
				class="avatar"
				src={pictureFailed ? avatar : `/api/profiles/${encodeURIComponent(target)}/picture`}
				alt={$language.t('profile.avatar')}
				width="96"
				height="96"
				onerror={() => {
					pictureFailed = true;
				}}
			/>{/key}
		<dl class="profile-details">
			<div>
				<dt>{$language.t('profile.email')}</dt>
				<dd>
					{$language.t('profile.emailValue', {
						email: profile.email,
						status: $language.t(profile.email_verified ? 'profile.verified' : 'profile.unverified')
					})}
				</dd>
			</div>
			<div>
				<dt>{$language.t('common.status')}</dt>
				<dd>{$language.t(profile.active ? 'common.active' : 'common.inactive')}</dd>
			</div>
			<div>
				<dt>{$language.t('profile.firstName')}</dt>
				<dd>{profile.first_name}</dd>
			</div>
			<div>
				<dt>{$language.t('profile.secondName')}</dt>
				<dd>{profile.second_name || $language.t('common.unspecified')}</dd>
			</div>
			<div>
				<dt>{$language.t('profile.lastName')}</dt>
				<dd>{profile.last_name}</dd>
			</div>
			<div>
				<dt>{$language.t('profile.secondLastName')}</dt>
				<dd>{profile.second_last_name || $language.t('common.unspecified')}</dd>
			</div>
			<div>
				<dt>{$language.t('profile.country')}</dt>
				<dd>
					{profile.country
						? `${countryLabel} (${profile.country})`
						: $language.t('common.unspecified')}
				</dd>
			</div>
			<div>
				<dt>{$language.t('profile.phone')}</dt>
				<dd>
					{profile.calling_code
						? `+${profile.calling_code} ${profile.national_number}`
						: $language.t('common.unspecified')}
				</dd>
			</div>
			<div>
				<dt>{$language.t('profile.language')}</dt>
				<dd>
					{profile.preferred_locale === 'es'
						? 'Español'
						: profile.preferred_locale === 'en'
							? 'English'
							: $language.t('profile.automatic')}
				</dd>
			</div>
			<div class="bio">
				<dt>{$language.t('profile.bio')}</dt>
				<dd>{profile.bio || $language.t('common.unspecified')}</dd>
			</div>
		</dl>
		{#if target === 'me'}<Button
				variant="outline"
				disabled={pending || blocked}
				onclick={() => {
					languageEditing = true;
				}}>{$language.t('profile.changeLanguage')}</Button
			>{/if}
	{/if}
	<div class="modal-actions">
		<Button variant="outline" disabled={pending} onclick={load}
			>{$language.t('profile.reload')}</Button
		>{#if profile}<Button disabled={pending || blocked} onclick={edit}
				>{$language.t('profile.edit')}</Button
			>{#if images}<Button
					variant="outline"
					disabled={pending || blocked}
					onclick={() => {
						pictureEditing = true;
					}}>{$language.t('profile.changePicture')}</Button
				>{/if}{/if}
	</div>
	{#if editing && profile && options}<Modal
			title={$language.t('profile.edit')}
			{pending}
			close={() => {
				editing = false;
			}}
			><ProfileEditor
				{profile}
				{options}
				{pending}
				{save}
				cancel={() => {
					editing = false;
				}}
			/></Modal
		>{/if}
	{#if pictureEditing && profile && images}<Modal
			title={$language.t('profile.picture')}
			{pending}
			close={() => {
				pictureEditing = false;
			}}
			><ImageControl
				cancel={() => {
					pictureEditing = false;
				}}
				api={images}
				path={`/api/profiles/${encodeURIComponent(target)}/picture`}
				revision={profile.revision}
				changed={() => {
					void load();
				}}
				busy={(value) => {
					pending = value;
				}}
			/></Modal
		>{/if}
</section>

{#if languageEditing && profile}<Modal
		title={$language.t('profile.accountLanguage')}
		{pending}
		close={() => {
			languageEditing = false;
		}}
		><LanguageEditor
			locale={profile.preferred_locale}
			{pending}
			save={saveLanguage}
			cancel={() => {
				languageEditing = false;
			}}
		/></Modal
	>{/if}

<style>
	.profile-panel .modal-actions {
		flex-wrap: wrap;
	}
	.avatar {
		border-radius: 1rem;
		object-fit: cover;
		margin-top: 1rem;
	}
	.glass.profile-panel {
		width: min(100%, 54rem);
		padding: clamp(1rem, 4vw, 2.5rem);
		justify-self: center;
	}
	h1 {
		font-size: 2rem;
	}
	.profile-details {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 1.2rem;
		margin: 1.5rem 0;
	}
	dt {
		color: #a9c6b4;
		font-size: 0.85rem;
	}
	dd {
		margin: 0.25rem 0;
		overflow-wrap: anywhere;
	}
	.bio {
		grid-column: 1/-1;
	}
	.bio dd {
		white-space: pre-wrap;
	}
	@media (max-width: 600px) {
		.profile-details {
			grid-template-columns: 1fr;
		}
	}
</style>
