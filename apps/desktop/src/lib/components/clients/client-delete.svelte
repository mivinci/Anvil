<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Dialog } from 'bits-ui';
	import { Trash2, LoaderCircle } from '@lucide/svelte';
	import { getClientRemovalState } from '../../client-removal-state.svelte.js';
	import { getClientLaunchState, launchActive } from '../../client-launch-state.svelte.js';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';
	let { name, disabled = false }: { name: string; disabled?: boolean } = $props();
	const removal = getClientRemovalState(), launches = getClientLaunchState(), installs = getClientInstallState(), resources = getResourceInstallState();
	let open = $state(false);
	$effect(() => { name; open = false; });
	let mounted = false;
	onMount(() => { mounted = true; return () => { mounted = false; }; });
	const statusUnknown = $derived(launches.uncertain || installs.uncertain || resources.uncertain);
	const inUse = $derived(launchActive(launches.forClient(name)) || ((installs.active || installs.starting) && installs.operation?.name.toLowerCase() === name.toLowerCase()) || ((resources.active || resources.starting) && resources.operation?.name.toLowerCase() === name.toLowerCase()));
	const blocked = $derived(disabled || statusUnknown || inUse || removal.busy || removal.uncertain);
	const pendingHere = $derived(removal.name?.toLowerCase() === name.toLowerCase());
	const secondary = 'h-8 rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	async function confirm() {
		if (blocked) return;
		const target = name;
		if (await removal.remove(target) && mounted && name === target) { open = false; await goto(resolve('/clients')); }
	}
	async function reconcile() {
		if (await removal.reconcile() && mounted) { open = false; await goto(resolve('/clients')); }
	}
</script>

<section aria-label={localeState.messages.clients.delete} class="mt-6 rounded-lg bg-card px-3.5 py-3">
	<div class="flex items-center justify-between gap-4">
		<div class="min-w-0"><h2 class="text-[13px] font-medium">{localeState.messages.clients.delete}</h2><p class="mt-1 text-[11px] leading-5 text-muted-foreground">{inUse ? localeState.messages.clients.deleteBusy : statusUnknown ? localeState.messages.clients.deleteChecking : localeState.messages.clients.deleteSummary}</p></div>
		<button type="button" disabled={blocked} onclick={() => { open = true; }} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2.5 text-xs text-destructive hover:bg-destructive/10 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Trash2 size={13} aria-hidden="true" />{localeState.messages.clients.delete}</button>
	</div>
	{#if removal.uncertain}<div role="alert" class="mt-2 text-[11px] leading-5 text-muted-foreground"><p>{pendingHere ? removal.error : localeState.messages.clients.deleteOther(removal.name)}</p><button type="button" class="mt-1 text-primary disabled:opacity-40" disabled={removal.busy} onclick={() => void reconcile()}>{localeState.messages.clients.deleteRefresh}</button></div>{/if}
</section>

<Dialog.Root open={open} onOpenChange={value => { if (!removal.busy) open = value; }}>
	<Dialog.Portal to=".app-window">
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
		<Dialog.Content class="fixed top-1/2 left-1/2 z-50 w-[min(360px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-popover p-4 text-popover-foreground shadow-lg outline-none" onEscapeKeydown={event => { if (removal.busy) event.preventDefault(); }} onInteractOutside={event => { if (removal.busy) event.preventDefault(); }}>
			<Dialog.Title class="break-words text-sm font-semibold">{localeState.messages.clients.deleteTitle(name)}</Dialog.Title>
			<Dialog.Description class="mt-2 text-xs leading-6 text-muted-foreground">{localeState.messages.clients.deleteWarning}</Dialog.Description>
			{#if pendingHere && removal.error}<p role="alert" class="mt-3 break-words text-xs leading-5 text-destructive">{removal.error}</p>{/if}
			{#if pendingHere && removal.errorDetail}<details class="mt-2 text-xs text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{removal.errorDetail}</p></details>{/if}
			<div class="mt-4 flex justify-end gap-1.5">
				<button type="button" class={secondary} disabled={removal.busy} onclick={() => { open = false; }}>{localeState.messages.clients.cancel}</button>
				{#if removal.uncertain}<button type="button" class={secondary} disabled={removal.busy} onclick={() => void reconcile()}>{localeState.messages.clients.deleteRefresh}</button>
				{:else}<button type="button" disabled={blocked} onclick={() => void confirm()} class="inline-flex h-8 items-center gap-1.5 rounded-lg bg-destructive px-3 text-xs font-medium text-white focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if removal.busy}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.clients.deleting}{:else}{localeState.messages.clients.deleteConfirm}{/if}</button>{/if}
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
