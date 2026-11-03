<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { onMount, tick } from 'svelte';
	import PageSidebar from '#lib/components/ui/page-sidebar.svelte';
	import { getPageSidebarState } from '../../page-sidebar-state.svelte.js';
	import { pageSidebarItemClass } from '../ui/page-sidebar-item';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Popover } from 'bits-ui';
	import { ChevronDown, Gamepad2, LoaderCircle, RefreshCw, Search, Sparkles } from '@lucide/svelte';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientDetailState } from '../../client-detail-state.svelte.js';
	import type { InstalledClient } from '../../runtime';
	import ShaderBrowser from './shader-browser.svelte';
	import SelectableList from '#lib/components/ui/selectable-list.svelte';
	import SelectableRow from '#lib/components/ui/selectable-row.svelte';

	const sidebar = getPageSidebarState();
	const library = getClientLibraryState();
	const detail = new ClientDetailState();
	const targetName = $derived(page.url.searchParams.get('instance'));
	let checkedName = $state<string | null>(null);
	const confirmed = $derived(detail.client && targetName && checkedName === targetName && detail.client.name.toLowerCase() === targetName.toLowerCase() ? detail.client : null);
	const targetIssue = $derived(targetName && checkedName === targetName && !detail.loading && detail.error ? detail.error
		: targetName && checkedName === targetName && detail.client && !confirmed ? localeState.messages.mods.discovery.targetMismatch : null);
	let pickerOpen = $state(false);
	let instanceSearch = $state('');
	let navigationErrorValue = $state<string | (() => string | null) | null>(null);
	const navigationError = $derived(typeof navigationErrorValue === 'function' ? navigationErrorValue() : navigationErrorValue);
	const filtered = $derived(library.clients.filter(c => `${c.name} ${c.versionId}`.toLowerCase().includes(instanceSearch.trim().toLowerCase())));
	const button = 'inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	async function chooseInstance() {
		sidebar.shaders = true;
		await tick();
		pickerOpen = true;
	}
	$effect(() => { if (!sidebar.shaders) pickerOpen = false; });
	onMount(() => { void library.refresh(); });
	$effect(() => {
		checkedName = null;
		if (!targetName) { detail.cancel(); detail.client = null; return; }
		const name = targetName;
		let active = true;
		void detail.load(name).then(() => { if (active) checkedName = name; });
		return () => { active = false; detail.cancel(); };
	});
	async function changeInstance(name: string | null) {
		pickerOpen = false;
		instanceSearch = '';
		navigationErrorValue = null;
		try {
			await goto(`${resolve('/shaders')}${name ? `?${new URLSearchParams({ instance: name })}` : ''}`, { reset: false });
		} catch { navigationErrorValue = () => (localeState.messages.mods.discovery.shaderNavigationFailed); }
	}
	function refreshInstances() {
		void library.refresh();
		if (targetName) void detail.load(targetName);
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">
	<PageSidebar open={sidebar.shaders} id="shader-discovery-sidebar" label={localeState.messages.mods.discovery.shaderSidebar}>
		<h1 class="px-2 pb-4 text-body font-semibold">{localeState.messages.mods.discovery.shaders}</h1>
		<div class="flex flex-col gap-4">
			<Popover.Root bind:open={pickerOpen}>
				<Popover.Trigger aria-label={targetName ? localeState.messages.mods.targetLabel(targetName) : localeState.messages.mods.discovery.noTargetLabel} class="inline-flex h-8 min-w-0 items-center gap-2 rounded-lg border border-input px-2.5 text-left text-xs focus-visible:outline-2 focus-visible:outline-ring">
					<Gamepad2 size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate">{confirmed?.name ?? targetName ?? localeState.messages.mods.discovery.chooseInstance}</span><ChevronDown size={12} class="shrink-0 text-muted-foreground" aria-hidden="true" />
				</Popover.Trigger>
				{#if pickerOpen}<Popover.Portal to=".app-window">
					<Popover.Content side="bottom" align="start" sideOffset={8} collisionPadding={12} class="z-30 w-80 max-w-[calc(100vw-32px)] rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg outline-none" aria-label={localeState.messages.mods.discovery.chooseShaderTarget}>
						<label class="mb-2 flex h-8 items-center gap-2 rounded-lg bg-chrome-hover px-2.5"><Search size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" /><input type="search" bind:value={instanceSearch} placeholder={localeState.messages.mods.discovery.searchInstanceHint} aria-label={localeState.messages.mods.discovery.searchTarget} class="min-w-0 flex-1 bg-transparent text-xs outline-none" /></label>
						{#if library.error}<p role="alert" class="mb-2 px-2 text-xs text-destructive">{library.loaded ? localeState.messages.mods.discovery.instancesRefreshFailed : localeState.messages.mods.discovery.instancesReadFailed}</p>{/if}
						<SelectableList class="max-h-64 overflow-y-auto overscroll-contain" aria-label={localeState.messages.mods.discovery.targetList} aria-busy={library.loading}>
							{#each filtered as item (item.id)}
								<SelectableRow selected={targetName === item.name.toLowerCase()} onclick={() => void changeInstance(item.name)} class="h-12" aria-label={localeState.messages.mods.instanceVersion(item.name, item.versionId)}>
									<span class="min-w-0"><span class="block truncate text-xs">{item.name}</span><span class="mt-1 block truncate text-xs text-muted-foreground">Minecraft {item.versionId}</span></span>
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
			<div class="flex flex-col gap-1" role="group" aria-label={localeState.messages.mods.discovery.shaderSource}>
				<button type="button" aria-pressed="true" class={pageSidebarItemClass(true)}><Sparkles size={15} strokeWidth={1.7} aria-hidden="true" />Modrinth</button>
			</div>
			<p class="px-2 text-xs leading-4 text-muted-foreground">{localeState.messages.mods.discovery.shaderScope}</p>
		</div>
	</PageSidebar>
	<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={localeState.messages.mods.discovery.shaderBrowsing}>
		<div class="flex min-h-0 flex-1 flex-col p-3 lg:p-4">
			<header class="mb-3 shrink-0"><h2 class="text-body font-semibold">{localeState.messages.mods.discovery.shaders}</h2><p class="mt-1 text-xs leading-4 text-muted-foreground">{localeState.messages.mods.discovery.shaderIntro}</p></header>
			{#if targetIssue}<div role="alert" class="mb-3 flex items-start gap-2 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs"><div class="min-w-0 flex-1"><p class="text-destructive">{targetIssue}</p><div class="mt-2 flex flex-wrap gap-1"><button type="button" class={button} onclick={() => void chooseInstance()}>{localeState.messages.mods.discovery.selectAgain}</button>{#if targetName}<button type="button" class={button} disabled={detail.loading} onclick={() => targetName && void detail.load(targetName)}>{localeState.messages.mods.discovery.reread}</button>{/if}</div></div></div>{/if}
			{#if navigationError}<p role="alert" class="mb-3 text-xs text-destructive">{navigationError}</p>{/if}
			{#if confirmed}
				{#key confirmed.id}<ShaderBrowser name={confirmed.name} gameVersion={confirmed.versionId} />{/key}
			{:else if targetName && detail.loading}
				<p role="status" class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.confirmingInstance}</p>
			{:else}
				<div class="flex flex-1 flex-col items-center justify-center gap-2 text-center">
					<Sparkles size={26} strokeWidth={1.4} class="text-primary/60" aria-hidden="true" />
					<p class="text-body">{localeState.messages.mods.discovery.chooseOne}</p>
					<p class="max-w-80 text-xs leading-4 text-muted-foreground">{localeState.messages.mods.discovery.shaderTargetHint}</p>
					<button type="button" onclick={() => void chooseInstance()} class="mt-1 inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.mods.discovery.chooseInstance}</button>
				</div>
			{/if}
		</div>
	</section>
</div>
