<script lang="ts">
	import { runtimeErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.js';
	import { untrack, tick } from 'svelte';
	import { ModProjectFeed, type ModProjectFeedSnapshot } from '../../mod-catalog-read';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { isTauri } from '@tauri-apps/api/core';
	import { ChevronRight, Search, RefreshCw, LoaderCircle } from '@lucide/svelte';
	import { shell } from '../../shell';
	import { ModCatalogState } from '../../mod-catalog-state.svelte';
	import ModProjectIcon from './mod-project-icon.svelte';
	import ModProjectStats from './mod-project-stats.svelte';
	import ModVersionInstaller from './mod-version-installer.svelte';
	import { getCurseForgeConfiguration, searchModProjects, getModProject,
		type ModSource, type ModProjectDetails } from '../../runtime';
	let { source, target, chooseInstance }: { source: ModSource; target: { name: string; version: string } | null; chooseInstance: () => void } = $props();
	let projects = $state<ModProjectFeedSnapshot>({ items: [], nextIndex: null, busy: false, error: null, warning: null, loaded: false });
	const feed = new ModProjectFeed(value => { projects = value; });
	let listElement = $state<HTMLDivElement>();
	let sentinel = $state<HTMLDivElement>();
	let savedScroll = 0;
	let installFooter = $state<HTMLDivElement>();
	const details = new ModCatalogState<ModProjectDetails>();
	const query = $derived(page.url.searchParams.get('q') ?? '');
	const index = $derived(Math.max(0, Number(page.url.searchParams.get('index')) || 0));
	const projectId = $derived(page.url.searchParams.get('project'));
	const validProject = $derived(projectId !== null && (source === 'modrinth' ? /^[a-zA-Z0-9]{8}$/.test(projectId) : /^[1-9][0-9]*$/.test(projectId)));
	let draft = $state('');
	let configured = $state(false);
	let configBusy = $state(false);
	let configDetail = $state<string | null>(null);
	let configErrorValue = $state<string | (() => string | null) | null>(null);
	const configError = $derived(typeof configErrorValue === 'function' ? configErrorValue() : configErrorValue);
	let configRevision = $state(0);
	let navigationErrorValue = $state<string | (() => string | null) | null>(null);
	const navigationError = $derived(typeof navigationErrorValue === 'function' ? navigationErrorValue() : navigationErrorValue);
	const iconButton = 'grid size-9 shrink-0 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	const button = 'rounded-lg px-2.5 py-1.5 text-xs hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	$effect(() => { draft = query; });
	$effect(() => {
		const current = source; configRevision;
		configured = false; configErrorValue = null; configDetail = null;
		if (current === 'modrinth') { configured = true; configBusy = false; return; }
		const controller = new AbortController(); configBusy = true;
		void getCurseForgeConfiguration(AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]))
			.then(value => { if (!controller.signal.aborted) { configured = value.configured; configDetail = value.issue; configErrorValue = value.configured ? null : () => localeState.messages.mods.discovery.notConfigured; } })
			.catch(error => { if (!controller.signal.aborted) { configDetail = String(error); configErrorValue = () => localeState.messages.mods.discovery.sourceCheckFailed; } })
			.finally(() => { if (!controller.signal.aborted) configBusy = false; });
		return () => controller.abort();
	});

	function loadDetails(force = false) {
		const current = source, id = projectId!;
		return details.load((signal, options) => getModProject(current, id, signal, options), force);
	}

	$effect(() => {
		source; query; index; target?.name; target?.version; const enabled = configured;
		const current = source, q = query, offset = index, version = target?.version;
		untrack(() => {
			savedScroll = 0;
			if (listElement) listElement.scrollTop = 0;
			feed.reset(enabled ? (cursor, signal, options) => searchModProjects(current, q, cursor, signal, { ...options, loader: 'fabric', ...(version ? { gameVersion: version } : {}) }) : null, offset);
		});
		return () => feed.cancel();
	});
	$effect(() => {
		const element = listElement;
		if (projectId || !element) return;
		let active = true;
		void tick().then(() => { if (active) element.scrollTop = savedScroll; });
		return () => { active = false; };
	});
	$effect(() => {
		const root = listElement, node = sentinel;
		const eligible = configured && !projects.busy && !projects.error && !projects.warning && projects.nextIndex !== null;
		projectId;
		if (!root || !node || !eligible) return;
		const observer = new IntersectionObserver(entries => {
			if (entries.some(entry => entry.isIntersecting) && root.getClientRects().length > 0) void feed.more();
		}, { root, rootMargin: '0px 0px 160px 0px' });
		observer.observe(node);
		return () => observer.disconnect();
	});

	$effect(() => {
		source; projectId; const enabled = configured && validProject;
		untrack(() => { details.clear(); if (enabled) void loadDetails(); });
		return () => details.cancel();
	});

	async function navigate(change: { q?: string; index?: number; project?: string | null }) {
		if (change.project && listElement) savedScroll = listElement.scrollTop;
		const next = new URL(page.url.href);
		for (const [key, value] of Object.entries(change)) {
			next.searchParams.delete(key);
			if (value !== null && value !== '' && value !== 0) next.searchParams.set(key, String(value));
		}
		navigationErrorValue = null;
		try { await goto(`${resolve('/mods')}${next.search}`, { reset: false }); }
		catch { navigationErrorValue = () => (localeState.messages.mods.discovery.navigationFailed); }
	}
	async function openSource() {
		try {
			const url = new URL(details.value?.project.links.websiteUrl ?? '');
			const hosts = source === 'modrinth' ? ['modrinth.com'] : ['curseforge.com', 'www.curseforge.com'];
			if (url.protocol !== 'https:' || !hosts.includes(url.hostname) || url.username || url.password || url.port) throw new Error(localeState.messages.mods.discovery.invalidSourceLink);
			if (isTauri()) await shell.openExternalUrl(url.href); else window.open(url.href, '_blank', 'noopener,noreferrer');
		} catch { navigationErrorValue = () => (localeState.messages.mods.discovery.openSourceFailed); }
	}
</script>

<div class="flex min-h-0 flex-1 flex-col">
	{#if !projectId}
	<form class="mb-3 flex shrink-0 items-center gap-2" onsubmit={event => { event.preventDefault(); void navigate({ q: draft.trim(), index: 0, project: null }); }}>
		<input type="search" aria-label={localeState.messages.mods.discovery.searchMods} placeholder={localeState.messages.mods.discovery.searchFabric} bind:value={draft} class="h-9 min-w-0 flex-1 rounded-lg bg-chrome-hover px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring" />
		<button type="submit" class={iconButton} aria-label={localeState.messages.mods.discovery.searchMods} title={localeState.messages.mods.discovery.searchMods}><Search size={16} strokeWidth={1.7} aria-hidden="true" /></button>
		<button type="button" class={iconButton} aria-label={localeState.messages.mods.discovery.refreshMods} title={localeState.messages.mods.discovery.refreshMods} disabled={projects.busy || !configured} onclick={() => void feed.refresh()}><RefreshCw size={15} strokeWidth={1.7} class={projects.busy ? "animate-spin motion-reduce:animate-none" : ""} aria-hidden="true" /></button>
	</form>
	{/if}
	{#if navigationError}<p role="alert" class="mb-3 text-xs text-destructive">{navigationError}</p>{/if}
	{#if configBusy}<p role="status" class="py-8 text-center text-xs text-muted-foreground">{localeState.messages.mods.discovery.checkingSource}</p>
	{:else if configError}<div role="alert" class="rounded-lg border border-border p-4 text-xs"><p>{configError}</p>{#if configDetail}<details class="mt-2"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{configDetail}</p></details>{/if}<button type="button" class={`${button} mt-2`} onclick={() => configRevision++}>{localeState.messages.mods.discovery.recheckSource}</button></div>
	{:else if configured}
	<div class="grid min-h-0 flex-1 grid-cols-1 gap-3 min-[1100px]:grid-cols-[minmax(240px,2fr)_minmax(0,3fr)]">
		<div class={`flex min-h-0 flex-col ${projectId ? 'hidden min-[1100px]:flex' : ''}`}>
			<div bind:this={listElement} onscroll={() => { if (!projectId && listElement?.getClientRects().length) savedScroll = listElement.scrollTop; }} tabindex="-1" role="region" class="resource-list resource-list-icons min-h-0 flex-1 scrollbar-subtle overflow-y-auto overscroll-contain focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring" aria-label={localeState.messages.mods.discovery.modProjects} aria-busy={projects.busy}>
				{#each projects.items as item (item.id)}
					<button type="button" aria-pressed={projectId === String(item.id)} onclick={() => void navigate({ project: String(item.id) })} class={`resource-row group flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring ${projectId === String(item.id) ? 'bg-chrome-selected' : 'hover:bg-chrome-hover'}`}>
						<ModProjectIcon project={item} {source} />
						<span class="min-w-0 flex-1"><span class="block truncate text-body font-medium leading-5" title={item.name}>{item.name}</span><span class="mt-0.5 block truncate text-xs leading-4 text-muted-foreground" title={item.summary}>{item.summary}</span></span>
						<ModProjectStats project={item} compact />
						<ChevronRight size={13} aria-hidden="true" class={`shrink-0 text-muted-foreground transition-opacity ${projectId === String(item.id) ? 'opacity-80' : 'opacity-30 group-hover:opacity-70 group-focus-visible:opacity-70'}`} />
					</button>
				{:else}{#if !projects.busy && !projects.error && !projects.warning}<p class="px-3 py-10 text-center text-xs text-muted-foreground">{localeState.messages.mods.discovery.noMods}</p>{/if}{/each}
				<div bind:this={sentinel} class="h-px" aria-hidden="true"></div>
				{#if projects.error || projects.warning}<p role="alert" class="px-3 py-2 text-xs text-destructive">{runtimeErrorMessage(projects.errorCode ?? projects.warningCode, localeState.current, localeState.messages.mods.discovery.catalogFailed)}{projects.items.length ? localeState.messages.mods.discovery.keepResults : ''}<button class={button} onclick={() => void feed.retry()} disabled={projects.busy}>{localeState.messages.mods.discovery.retry}</button></p><details class="px-3 py-2 text-xs text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{projects.error ?? projects.warning}</p></details>{/if}
				{#if projects.busy}<p role="status" class="flex items-center justify-center gap-2 py-3 text-xs text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{projects.loaded ? localeState.messages.mods.discovery.loadingMore : localeState.messages.mods.discovery.loadingMods}</p>
				{:else if projects.nextIndex !== null && !projects.error && !projects.warning}<button type="button" class={`${button} block w-full text-muted-foreground`} onclick={() => void feed.more()}>{localeState.messages.mods.discovery.loadMore}</button>
				{:else if projects.items.length && !projects.error && !projects.warning}<p role="status" class="py-3 text-center text-xs text-muted-foreground">{localeState.messages.mods.discovery.allResults}</p>{/if}
			</div>
		</div>
		<div class={`flex min-h-0 flex-col overflow-hidden ${!projectId ? 'hidden min-[1100px]:flex' : ''}`}>
			<div class="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden" role="region" aria-label={localeState.messages.mods.discovery.modDetails}>
			{#if projectId}
				<section aria-label={localeState.messages.mods.discovery.modIntro} class="max-h-[45%] shrink-0 scrollbar-subtle overflow-y-auto overscroll-contain px-2.5 pt-1 pb-3">
				{#if !validProject}<p role="alert" class="text-xs text-destructive">{localeState.messages.mods.discovery.invalidProject}</p>{/if}
				{#if details.busy}<p role="status" class="text-xs text-muted-foreground">{localeState.messages.mods.discovery.loadingProject}</p>{/if}
				{#if details.error || details.value?.refreshError}<p role="alert" class="mb-3 text-xs text-destructive">{runtimeErrorMessage(details.errorCode ?? details.value?.refreshErrorCode, localeState.current, localeState.messages.mods.discovery.catalogFailed)}{details.value ? localeState.messages.mods.discovery.keepSummary : ''}<button type="button" class={button} disabled={details.busy} onclick={() => void loadDetails(true)}>{localeState.messages.mods.discovery.retry}</button></p><details class="px-3 py-2 text-xs text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{details.errorDetail ?? details.value?.refreshError}</p></details>{/if}
				{#if details.value}
					<div class="flex flex-wrap items-start justify-between gap-3"><div class="flex min-w-0 flex-1 items-center gap-3"><ModProjectIcon project={details.value.project} {source} large /><div class="min-w-0"><h2 class="break-words text-lg font-semibold">{details.value.project.name}</h2>
					{#if details.value.project.authors.length}<p class="mt-1 break-words text-xs text-muted-foreground">{details.value.project.authors.map(author => author.name).join(localeState.current === 'zh-CN' ? '、' : ', ')}</p>{/if}</div></div>
					{#if details.value.project.links.websiteUrl}<button class={`${button} ml-auto shrink-0`} onclick={() => void openSource()}>{localeState.messages.mods.viewSource(source === 'modrinth' ? 'Modrinth' : 'CurseForge')}</button>{/if}</div>
					<ModProjectStats project={details.value.project} />
					<p class="mt-3 whitespace-pre-wrap break-words text-xs leading-6 text-muted-foreground">{details.value.project.summary}</p>
				{/if}
				</section>
				{#if validProject}
				<section aria-label={localeState.messages.mods.discovery.compatibleVersions} class={`flex min-h-0 flex-col ${target ? 'flex-1' : 'shrink-0'}`}><div class="mx-2.5 flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-border/60 pb-2"><h3 class="text-body font-medium">{localeState.messages.mods.discovery.compatibleVersions}</h3>{#if target}<p class="min-w-0 break-words text-xs text-muted-foreground">Minecraft {target.version} · Fabric · {target.name}</p>{/if}</div>
				{#if !target}<p class="mt-2 text-xs leading-5 text-muted-foreground">{localeState.messages.mods.discovery.chooseForVersions}</p><button class={`${button} mt-2`} onclick={chooseInstance}>{localeState.messages.mods.discovery.chooseInstance}</button>
				{:else}
					<ModVersionInstaller {source} name={target.name} gameVersion={target.version} project={projectId!} footerTarget={installFooter} />
				{/if}</section>{/if}
			{:else}<p class="py-16 text-center text-xs text-muted-foreground">{localeState.messages.mods.discovery.chooseMod}</p>{/if}
			</div>
			<div bind:this={installFooter} class="shrink-0 overflow-hidden rounded-lg border-border empty:hidden [&:not(:empty)]:mt-3 [&:not(:empty)]:border"></div>
		</div>
	</div>{/if}
</div>
