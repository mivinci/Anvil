<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { onMount, tick } from 'svelte';
	import PageSidebar from '#lib/components/ui/page-sidebar.svelte';
	import { pageSidebarItemClass } from '../ui/page-sidebar-item';
	import { getPageSidebarState } from '../../page-sidebar-state.svelte.js';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Popover } from 'bits-ui';
	import { Blocks, Flame, ChevronDown, Gamepad2, LoaderCircle, RefreshCw, Search, TriangleAlert } from '@lucide/svelte';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientDetailState } from '../../client-detail-state.svelte.js';
	import { readModDiscoveryContext, updateModDiscoveryContext } from '../../mod-discovery-context';
	import type { InstalledClient, ModSource } from '../../runtime';
	import ModProjectBrowser from './mod-project-browser.svelte';
	import SelectableList from '#lib/components/ui/selectable-list.svelte';
	import SelectableRow from '#lib/components/ui/selectable-row.svelte';

	const sidebar = getPageSidebarState();
	const library = getClientLibraryState();
	const detail = new ClientDetailState();
	const context = $derived(readModDiscoveryContext(page.url));
	const source = $derived(context.source);
	const targetName = $derived(context.instanceName);
	let checkedName = $state<string | null>(null);
	const client = $derived(detail.client);
	const confirmed = $derived(client && targetName && checkedName === targetName && client.name.toLowerCase() === targetName.toLowerCase() ? client : null);
	const targetIssue = $derived(context.instanceIssue ? localeState.messages.mods.discovery.invalidTarget
		: targetName && checkedName === targetName && !detail.loading && detail.error ? detail.error
		: targetName && checkedName === targetName && client && !confirmed ? localeState.messages.mods.discovery.targetMismatch
		: confirmed && !detail.loading && !fabric(confirmed) ? localeState.messages.mods.discovery.requiresFabric : null);
	const ready = $derived(confirmed && !detail.loading && !targetIssue && fabric(confirmed) ? confirmed : null);
	let pickerOpen = $state(false);
	let instanceSearch = $state('');
	let navigating = $state(false);
	let navigationErrorValue = $state<string | (() => string | null) | null>(null);
	const navigationError = $derived(typeof navigationErrorValue === 'function' ? navigationErrorValue() : navigationErrorValue);
	const filtered = $derived(library.clients.filter(c => `${c.name} ${c.versionId}`.toLowerCase().includes(instanceSearch.trim().toLowerCase())));
	const button = 'inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	function fabric(value: InstalledClient) {
		return value.loader?.kind === 'fabric' && typeof value.loader.version === 'string' && value.loader.version.length > 0;
	}
	async function chooseInstance() { sidebar.mods = true; await tick(); pickerOpen = true; }
	$effect(() => { if (!sidebar.mods) pickerOpen = false; });
	onMount(() => { void library.refresh(); });
	$effect(() => {
		checkedName = null;
		if (!targetName) { detail.cancel(); detail.client = null; return; }
		const name = targetName;
		let active = true;
		void detail.load(name).then(() => { if (active) checkedName = name; });
		return () => { active = false; detail.cancel(); };
	});
	async function changeContext(change: { instance?: string | null; source?: ModSource }) {
		if (navigating) return;
		pickerOpen = false;
		instanceSearch = '';
		navigationErrorValue = null;
		navigating = true;
		try {
			const next = updateModDiscoveryContext(page.url, change);
			if (next.search !== page.url.search) await goto(`${resolve('/mods')}${next.search}`, { reset: false });
		} catch { navigationErrorValue = () => (localeState.messages.mods.discovery.modNavigationFailed); }
		finally { navigating = false; }
	}
	function refreshInstances() {
		void library.refresh();
		if (targetName) void detail.load(targetName);
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">
	<PageSidebar open={sidebar.mods} id="mod-discovery-sidebar" label={localeState.messages.mods.discovery.modSidebar}>
		<h1 class="px-2 pb-4 text-[13px] font-semibold">{localeState.messages.mods.discovery.mods}</h1>
		<div class="flex flex-col gap-4">
			<div class="flex min-w-0 items-center gap-1.5">
				<Popover.Root bind:open={pickerOpen}>
					<Popover.Trigger disabled={navigating} aria-label={targetName ? localeState.messages.mods.targetLabel(targetName) : localeState.messages.mods.discovery.noTargetLabel} class="inline-flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg border border-input px-2.5 text-left text-xs focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">
						<Gamepad2 size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate">{confirmed?.name ?? targetName ?? (context.hasInstance ? localeState.messages.mods.discovery.invalidInstance : localeState.messages.mods.discovery.chooseInstance)}</span><ChevronDown size={12} class="shrink-0 text-muted-foreground" aria-hidden="true" />
					</Popover.Trigger>
					{#if pickerOpen}<Popover.Portal to=".app-window">
					<Popover.Content side="bottom" align="start" sideOffset={8} collisionPadding={12} class="z-30 w-80 max-w-[calc(100vw-32px)] rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg outline-none" aria-label={localeState.messages.mods.discovery.chooseModTarget}>
						<label class="mb-2 flex h-8 items-center gap-2 rounded-lg bg-chrome-hover px-2.5"><Search size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" /><input type="search" bind:value={instanceSearch} placeholder={localeState.messages.mods.discovery.searchInstanceHint} aria-label={localeState.messages.mods.discovery.searchTarget} class="min-w-0 flex-1 bg-transparent text-xs outline-none" /></label>
						{#if library.error}<p role="alert" class="mb-2 px-2 text-[12px] text-destructive">{library.loaded ? localeState.messages.mods.discovery.instancesRefreshFailed : localeState.messages.mods.discovery.instancesReadFailed}</p>{/if}
						<SelectableList class="max-h-64 overflow-y-auto overscroll-contain" aria-label={localeState.messages.mods.discovery.targetList} aria-busy={library.loading}>
							<SelectableRow selected={!context.hasInstance} onclick={() => void changeContext({ instance: null })} class="h-9"><span class="text-xs">{localeState.messages.mods.discovery.noInstance}</span>{#snippet trailing()}{localeState.messages.mods.discovery.browseOnly}{/snippet}</SelectableRow>
							{#each filtered as item (item.id)}
								<SelectableRow selected={targetName === item.name.toLowerCase()} disabled={!fabric(item) || navigating} onclick={() => void changeContext({ instance: item.name })} class="h-12" aria-label={localeState.messages.mods.instanceLabel(item.name, item.versionId, fabric(item) ? 'Fabric' : localeState.messages.mods.discovery.unsupportedMods)}>
									<span class="min-w-0"><span class="block truncate text-xs">{item.name}</span><span class="mt-1 block truncate text-[12px] text-muted-foreground">Minecraft {item.versionId}{fabric(item) ? ` · Fabric ${item.loader?.version}` : localeState.messages.mods.discovery.noFabricSuffix}</span></span>
								</SelectableRow>
							{/each}
						</SelectableList>
						{#if !library.loaded && library.loading}<p role="status" class="flex items-center justify-center gap-2 py-4 text-xs text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.loadingInstances}</p>
						{:else if library.loaded && !library.clients.length}<p class="px-2 py-4 text-xs text-muted-foreground">{localeState.messages.mods.discovery.noInstances}</p>
						{:else if library.loaded && !filtered.length}<p class="px-2 py-4 text-xs text-muted-foreground">{localeState.messages.mods.discovery.noMatchingInstances}</p>{/if}
						<div class="mt-2 flex justify-between border-t border-border pt-2"><a href={resolve('/clients')} class={button}>{localeState.messages.mods.discovery.manageClients}</a><button type="button" class={button} disabled={library.loading} onclick={refreshInstances}><RefreshCw size={12} class={library.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" />{localeState.messages.mods.discovery.refreshInstances}</button></div>
					</Popover.Content>
					</Popover.Portal>{/if}
				</Popover.Root>
			</div>
			<div class="flex flex-col gap-1" role="group" aria-label={localeState.messages.mods.discovery.modSource}>
				{#each [{ id: 'modrinth', name: 'Modrinth', icon: Blocks }, { id: 'curseforge', name: 'CurseForge', icon: Flame }] as source}
					<button type="button" aria-pressed={context.source === source.id} disabled={navigating} onclick={() => void changeContext({ source: source.id as ModSource })} class={pageSidebarItemClass(context.source === source.id)}><source.icon size={15} strokeWidth={1.7} aria-hidden="true" />{source.name}</button>
				{/each}
			</div>
		</div>
	</PageSidebar>
	<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={localeState.messages.mods.discovery.modBrowsing}>
		<div class="flex min-h-0 flex-1 flex-col p-3 lg:p-4">
			{#if context.sourceIssue}<div role="alert" class="mb-3 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs text-destructive">{localeState.messages.mods.discovery.invalidSource}</div>{/if}
			{#if targetIssue}<div role="alert" class="mb-3 flex items-start gap-2 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs"><TriangleAlert size={14} class="mt-0.5 shrink-0 text-destructive" aria-hidden="true" /><div class="min-w-0 flex-1"><p class="text-destructive">{targetIssue}</p><div class="mt-2 flex flex-wrap gap-1"><button type="button" class={button} disabled={navigating} onclick={() => void chooseInstance()}>{localeState.messages.mods.discovery.selectAgain}</button>{#if targetName}<button type="button" class={button} disabled={detail.loading} onclick={() => targetName && void detail.load(targetName)}>{localeState.messages.mods.discovery.reread}</button>{/if}<button type="button" class={button} disabled={navigating} onclick={() => void changeContext({ instance: null })}>{localeState.messages.mods.discovery.browseMods}</button></div></div></div>{/if}
			{#if navigationError}<p role="alert" class="mb-3 text-xs text-destructive">{navigationError}</p>{/if}
			{#if source && (!context.hasInstance || ready)}
				<ModProjectBrowser {source} target={ready ? { name: targetName!, version: ready.versionId } : null} chooseInstance={() => void chooseInstance()} />
			{:else if !targetIssue && context.hasInstance}
				<p role="status" class="py-10 text-center text-xs text-muted-foreground">{localeState.messages.mods.discovery.waitingTarget}</p>
			{/if}
		</div>
	</section>
</div>
