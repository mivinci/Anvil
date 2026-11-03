<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { resolve } from '$app/paths';
	import { Gamepad2, LoaderCircle, Plus, RefreshCw } from '@lucide/svelte';
	import type { PlayStatsAccount } from '../runtime';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState, launchActive, launchLabel } from '../client-launch-state.svelte.js';
	import { formatPlayDuration } from '../play-stats';
	import ClientLaunchControls from './clients/client-launch-controls.svelte';

	let { history, historyLoading = false, useDeviceHistory = false }: {
		history: PlayStatsAccount | null; historyLoading?: boolean; useDeviceHistory?: boolean;
	} = $props();
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
	let now = $state(Date.now());
	const active = $derived(launches.operations.filter(launchActive).sort((a, b) =>
		Number(b.phase === 'running') - Number(a.phase === 'running') || b.id - a.id));
	const operation = $derived(active[0]);
	const recent = $derived.by(() => {
		const candidates = Object.entries(history?.instances ?? {}).map(([id, value]) => ({ id, at: value.lastPlayedAt }));
		if (useDeviceHistory) candidates.push(...launches.recentPlays.map(value => ({ id: value.name, at: value.playedAt })));
		return candidates.sort((a, b) => b.at - a.at).map(value => library.clients.find(client =>
			client.id === value.id || client.name.toLowerCase() === value.id.toLowerCase())).find(Boolean);
	});
	const pending = $derived(library.clients.find(client => launches.pending.includes(client.name)));
	const client = $derived(library.clients.find(value => value.name.toLowerCase() === operation?.name.toLowerCase()) ?? pending ?? recent);
	const name = $derived(operation?.name ?? client?.name);
	const running = $derived(operation?.phase === 'running');
	const status = $derived(launches.uncertain ? localeState.messages.home.flow.runningUnconfirmed : operation
		? running ? `${operation.offlineTest ? localeState.messages.home.flow.offlinePlaying : localeState.messages.home.flow.running}${operation.startedAt ? ` · ${formatPlayDuration(now - operation.startedAt, localeState.current)}` : ''}` : launchLabel(operation)
		: pending ? localeState.messages.home.flow.preparingLaunch : localeState.messages.home.flow.recentPlay);
	const iconAction = 'grid size-7 shrink-0 place-items-center rounded-lg text-primary transition-colors hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-ring';

	$effect(() => {
		if (!running) return;
		now = Date.now();
		const timer = setInterval(() => { now = Date.now(); }, 30_000);
		return () => clearInterval(timer);
	});
</script>

<div class="mt-3 grid min-h-12 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-2 rounded-lg bg-primary/8 px-2.5 py-2 text-[11px]" aria-label={localeState.messages.home.flow.quickPlay}>
	{#if name}
		<a href={resolve('/clients/[name]', { name: name.toLowerCase() })} class="min-w-0 rounded focus-visible:outline-2 focus-visible:outline-ring" title={name}>
			<span class="flex items-center gap-1.5">
				<span class={`size-1.5 shrink-0 rounded-full ${launches.uncertain ? 'bg-muted-foreground/50' : running ? 'bg-emerald-500' : 'bg-primary/60'}`} aria-hidden="true"></span>
				<span class="truncate font-medium">{client?.name ?? name}</span>
			</span>
			<span class="mt-0.5 ml-3 block truncate text-[10px] text-muted-foreground" role="status" title={status}>{status}</span>
		</a>
		{#if launches.uncertain}
			<button type="button" class={iconAction} onclick={() => void launches.sync()} aria-label={localeState.messages.home.flow.refreshRunning} title={localeState.messages.home.flow.refreshRunning}><RefreshCw size={13} aria-hidden="true" /></button>
		{:else if running || operation?.phase === 'stopping' || (operation && !client)}
			<a href={resolve('/clients/[name]/logs', { name: name.toLowerCase() })} class="rounded-md px-1 py-1 text-primary hover:underline focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.home.flow.logs}</a>
		{:else if client}
			<ClientLaunchControls {client} compact feedback reserveCancel={false} />
		{/if}
		{#if active.length > 1}<a href={resolve('/clients')} class="col-span-2 mt-1 ml-3 w-fit rounded text-[10px] text-primary hover:underline">{localeState.messages.clients.otherActive(active.length - 1)}</a>{/if}
	{:else if library.error}
		<span class="text-muted-foreground">{localeState.messages.home.flow.instanceUnavailable}</span><button type="button" class={iconAction} onclick={() => void library.refresh()} aria-label={localeState.messages.home.flow.reloadInstances}><RefreshCw size={13} aria-hidden="true" /></button>
	{:else if !library.loaded || historyLoading}
		<span class="text-muted-foreground" role="status">{localeState.messages.home.flow.loadingRecent}</span><LoaderCircle size={13} class="animate-spin text-muted-foreground motion-reduce:animate-none" aria-hidden="true" />
	{:else}
		<div class="min-w-0"><p class="font-medium">{library.clients.length ? localeState.messages.home.flow.chooseInstance : localeState.messages.home.flow.firstPlay}</p><p class="mt-0.5 text-[10px] text-muted-foreground">{library.clients.length ? localeState.messages.home.flow.continueWorld : localeState.messages.home.flow.installFirst}</p></div>
		<a href={resolve('/clients')} class={iconAction} aria-label={library.clients.length ? localeState.messages.home.flow.openLibrary : localeState.messages.home.flow.installGame} title={library.clients.length ? localeState.messages.home.flow.openLibrary : localeState.messages.home.flow.installGame}>{#if library.clients.length}<Gamepad2 size={15} aria-hidden="true" />{:else}<Plus size={15} aria-hidden="true" />{/if}</a>
	{/if}
</div>
