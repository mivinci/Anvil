<script lang="ts">
	import { Camera, LoaderCircle } from '@lucide/svelte';
	import { localeState } from '#lib/locale.svelte.ts';
	import { RuntimeRequestError } from '#lib/runtime.ts';
	import { getClientAvatarState } from '../../client-avatar-state.svelte.js';
	import ClientAvatar from './client-avatar.svelte';
	import ImageCropDialog from '../ui/image-crop-dialog.svelte';

	let {
		id,
		name,
		class: className = 'size-16',
		showRestore = false,
		showStatus = true,
	}: {
		id: string;
		name: string;
		class?: string;
		showRestore?: boolean;
		showStatus?: boolean;
	} = $props();
	const avatars = getClientAvatarState();
	const messages = $derived(localeState.messages.instance.detail);
	const view = $derived(avatars.view(name));
	let fileInput: HTMLInputElement;
	let selectedFile = $state<File | null>(null);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let errorDetail = $state<string | null>(null);
	let notice = $state<string | null>(null);

	function choose() {
		if (saving) return;
		error = null;
		errorDetail = null;
		notice = null;
		fileInput.value = '';
		fileInput.click();
	}

	function selected(event: Event) {
		const input = event.currentTarget;
		if (!(input instanceof HTMLInputElement)) return;
		const file = input.files?.[0] ?? null;
		input.value = '';
		if (file) {
			error = null;
			errorDetail = null;
			notice = null;
			selectedFile = file;
		}
	}

	function failure(cause: unknown, fallback: string): string {
		if (cause instanceof RuntimeRequestError) {
			errorDetail = `${cause.status}${cause.code ? ` ${cause.code}` : ''}: ${cause.message}`;
			if (cause.status === 404 || cause.status === 405) return messages.avatarRuntimeOutdated;
			if (cause.status === 409 || cause.status === 429) return messages.avatarBusy;
			if (cause.status === 413) return messages.avatarTooLarge;
			if (cause.status === 400) return messages.avatarInvalid;
			return fallback;
		}
		errorDetail = cause instanceof Error ? cause.message : String(cause);
		if (cause instanceof TypeError) return messages.avatarUnavailable;
		return fallback;
	}

	async function save(blob: Blob) {
		if (saving) return;
		saving = true;
		error = null;
		errorDetail = null;
		notice = null;
		try {
			await avatars.upload(name, blob);
			notice = messages.avatarSaved;
		} catch (cause) {
			error = failure(cause, messages.avatarSaveFailed);
		} finally {
			saving = false;
			selectedFile = null;
		}
	}

	async function restore() {
		if (saving) return;
		saving = true;
		error = null;
		errorDetail = null;
		notice = null;
		try {
			await avatars.clear(name);
			notice = messages.avatarRestored;
		} catch (cause) {
			error = failure(cause, messages.avatarRestoreFailed);
		} finally {
			saving = false;
		}
	}

	function cancelCrop() {
		if (!saving) selectedFile = null;
	}
</script>

<div class="inline-flex min-w-0 flex-col items-start gap-1">
	<button
		type="button"
		onclick={choose}
		disabled={saving}
		aria-label={messages.changeAvatar}
		title={messages.changeAvatar}
		aria-busy={saving}
		class={`group relative shrink-0 overflow-hidden rounded-xl focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:opacity-70 ${className}`}
	>
		<ClientAvatar {id} {name} class="size-full" />
		<span class="pointer-events-none absolute right-0.5 bottom-0.5 grid size-5 place-items-center rounded-full bg-black/55 text-white shadow-sm" aria-hidden="true">
			{#if saving}<LoaderCircle size={11} class="animate-spin motion-reduce:animate-none" />{:else}<Camera size={11} />{/if}
		</span>
	</button>
	<input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/webp" aria-label={messages.changeAvatar} class="hidden" onchange={selected} />
	{#if showRestore && view.custom}
		<button type="button" disabled={saving} onclick={restore} class="max-w-full text-left text-[10px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{messages.restoreAvatar}</button>
	{/if}
	{#if showStatus && error}<p role="alert" class="max-w-48 text-[10px] leading-4 text-destructive">{error}</p>{/if}
	{#if showStatus && errorDetail}<details class="max-w-48 text-[10px] leading-4 text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="mt-1 select-text break-all">{errorDetail}</p></details>{/if}
	{#if showStatus && notice}<p role="status" class="max-w-48 text-[10px] leading-4 text-muted-foreground">{notice}</p>{/if}
</div>

{#if selectedFile}<ImageCropDialog file={selectedFile} onCancel={cancelCrop} onConfirm={save} />{/if}
