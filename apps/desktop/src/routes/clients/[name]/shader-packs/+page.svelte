<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { untrack } from 'svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Sparkles, FileArchive, LoaderCircle, Plus, RefreshCw, Search, Trash2, X, CircleCheck, TriangleAlert } from '@lucide/svelte';
	import { ClientShaderPackState } from '#lib/client-shader-pack-state.svelte.js';
	import { localPackSize } from '#lib/local-pack.js';
	import { shaderEnvironmentLabels, shaderEnvironmentReason } from '#lib/shader-packs.js';
	import { getResourceInstallState } from '#lib/resource-install-state.svelte.js';
	import { getClientLaunchState, launchActive } from '#lib/client-launch-state.svelte.js';
	import ModVersionInstaller from '#lib/components/mods/mod-version-installer.svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	let installerOpen = $state(false);
	let installerFooter = $state<HTMLElement>();
	const packs = new ClientShaderPackState();
	const launches = getClientLaunchState();
	const detail = getClientDetailState();
	const resources = getResourceInstallState();
	let trackedTask: number | null = null;
	const name = $derived(page.params.name ?? '');
	const running = $derived(launchActive(launches.forClient(name)));
	const installing = $derived((resources.active && resources.operation?.name.toLowerCase() === name.toLowerCase()) || (resources.starting && resources.intent?.name.toLowerCase() === name.toLowerCase()));
	const locked = $derived(packs.busy || packs.blocked || running || installing || installerOpen);
	const modsHref = $derived(resolve('/clients/[name]/mods', { name }));
	const environmentMods = $derived([{ label: 'Iris', mod: packs.environment?.iris }, { label: 'Sodium', mod: packs.environment?.sodium }]);
	const applied = $derived(packs.collection !== null && packs.collection.appliedRevision === packs.collection.revision);
	let search = $state('');
	let input: HTMLInputElement;
	const visible = $derived(packs.collection?.packs.filter(pack => pack.filename.toLowerCase().includes(search.trim().toLowerCase())) ?? []);
	const secondary = 'inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	$effect(() => {
		const target = name;
		untrack(() => { search = ''; installerOpen = false; trackedTask = null; packs.open(target); });
		return () => packs.dispose();
	});
	$effect(() => {
		const task = resources.operation, target = name;
		untrack(() => {
			if (!task || task.name.toLowerCase() !== target.toLowerCase()) return;
			if (task.state === 'running') trackedTask = task.id;
			else if (trackedTask === task.id) { trackedTask = null; packs.queueRefresh(); }
		});
	});
	function pick(event: Event) {
		const element = event.currentTarget as HTMLInputElement, file = element.files?.[0];
		element.value = '';
		if (file && !locked) packs.pick(file);
	}
</script>

<svelte:head><title>{localeState.messages.common.instanceTitle(name, localeState.messages.nav.shaders)}</title></svelte:head>
<section aria-labelledby="shader-packs-title" class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	<header class="flex shrink-0 flex-wrap items-center justify-between gap-2">
		<div class="flex items-baseline gap-2"><h1 id="shader-packs-title" class="text-base font-semibold tracking-tight">{localeState.messages.instance.detail.shaders}</h1>{#if packs.collection}<span class="text-[12px] text-muted-foreground">{localeState.messages.instance.count(packs.collection.packs.length, 'items')}</span>{/if}</div>
		<div class="flex items-center gap-1.5">
			{#if packs.collection?.packs.length}<button type="button" disabled={locked} onclick={() => void packs.sync()} title={localeState.messages.instance.detail.syncManaged} class="h-7 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">{packs.activity === 'syncing' ? localeState.messages.instance.detail.syncing : applied ? localeState.messages.instance.detail.syncFiles : localeState.messages.instance.detail.applyDirectory}</button>{/if}
			<button type="button" disabled={packs.busy} onclick={() => void packs.refresh()} aria-label={localeState.messages.instance.detail.refreshShaders} title={localeState.messages.instance.detail.refreshList} class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={packs.busy ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
			<button type="button" disabled={locked || !packs.collection} onclick={() => input.click()} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">{localeState.messages.instance.detail.localImport}</button>
			<a href={`${resolve('/shaders')}?${new URLSearchParams({ instance: name.toLowerCase() })}`} aria-label={localeState.messages.instance.addShaderFor(name)} title={localeState.messages.instance.detail.browseShaders} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring"><Plus size={13} aria-hidden="true" />{localeState.messages.instance.detail.addShader}</a>
		</div>
	</header>
	<input bind:this={input} type="file" accept=".zip,application/zip" onchange={pick} aria-label={localeState.messages.instance.detail.chooseShaderZip} class="hidden" />
	<p class="mt-2 text-[12px] leading-5 text-muted-foreground">{localeState.messages.instance.detail.shaderDescription}</p>
	<aside aria-labelledby="shader-environment-title" class="mt-3 rounded-lg bg-card px-3 py-2.5">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<h2 id="shader-environment-title" class="flex items-center gap-1.5 text-xs font-medium">{packs.environment ? shaderEnvironmentLabels(localeState.current)[packs.environment.status] : packs.busy ? localeState.messages.instance.detail.loadingEnvironment : localeState.messages.instance.detail.environmentUnknown}{#if packs.environment?.status === 'ready'}<CircleCheck size={14} class="shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden="true" />{:else if packs.environment}<TriangleAlert size={14} class="shrink-0 text-amber-600 dark:text-amber-400" aria-hidden="true" />{/if}</h2>
			{#if packs.environment?.status === 'requires_fabric'}<a href={resolve('/clients')} class="rounded text-[12px] text-primary underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.instance.detail.openClients}</a>
			{:else if packs.environment?.status === 'missing_iris'}<button type="button" disabled={packs.busy || packs.blocked || running || installing || resources.starting} onclick={() => installerOpen = !installerOpen} class="rounded text-[12px] text-primary underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{installerOpen ? localeState.messages.instance.detail.hideInstaller : localeState.messages.instance.detail.installShaders}</button>
			{:else if packs.environment}<a href={modsHref} class="rounded text-[12px] text-primary underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.instance.detail.manageMods}</a>{/if}
		</div>
		{#if packs.environment}
			<details class="mt-1 text-[12px] text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{packs.environment.reason}</p></details>
			{#if packs.environment.iris || packs.environment.sodium}<div class="mt-2 flex flex-wrap gap-1.5 text-[12px] text-muted-foreground">{#each environmentMods as { label, mod }}{#if mod}<span class="max-w-full break-all rounded bg-chrome-hover px-1.5 py-0.5">{label} {mod.version}{mod.enabled ? '' : localeState.messages.instance.disabledSuffix}</span>{/if}{/each}</div>{/if}
			<p class="mt-1.5 wrap-break-word text-[12px] leading-5 text-muted-foreground">{shaderEnvironmentReason(packs.environment, localeState.current)}</p>
			<p class="mt-1 text-[12px] text-muted-foreground">{localeState.messages.instance.detail.managedOnly}</p>
		{:else if !packs.busy}<p class="mt-1.5 text-[12px] leading-5 text-muted-foreground">{localeState.messages.instance.detail.refreshEnvironment}</p>{/if}
	</aside>
	{#if installerOpen && detail.client}
		<section aria-label={localeState.messages.instance.detail.installIris} class="mt-3 shrink-0 overflow-hidden rounded-lg bg-card">
			<div class="flex items-center justify-between px-3 pt-3"><h2 class="text-xs font-medium">Iris · Minecraft {detail.client.versionId}</h2><button type="button" class={secondary} onclick={() => installerOpen = false} aria-label={localeState.messages.instance.detail.closeIris}><X size={13} aria-hidden="true" /></button></div>
			<p class="px-3 pt-1 text-[12px] leading-5 text-muted-foreground">{localeState.messages.instance.detail.irisInstallDescription}</p>
			<div class="flex h-48 min-h-0 flex-col px-3 pb-2"><ModVersionInstaller source="modrinth" {name} gameVersion={detail.client.versionId} project="YL57xq9U" footerTarget={installerFooter} blocked={packs.busy || packs.blocked || running} /></div>
			<div bind:this={installerFooter}></div>
		</section>
	{/if}
	{#if installing}<p role="status" class="mt-2 text-[12px] text-muted-foreground">{localeState.messages.instance.detail.modTaskRunning}</p>{/if}
	{#if running}<p role="status" class="mt-2 text-[12px] text-muted-foreground">{localeState.messages.instance.detail.shaderRunning}</p>{/if}
	{#if packs.error}<div role="alert" class="mt-3 rounded-lg bg-destructive/5 px-3 py-2 text-xs text-destructive"><p class="wrap-break-word">{packs.error}</p>{#if packs.errorDetail}<details class="mt-1 text-[12px] text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{packs.errorDetail}</p></details>{/if}<button type="button" class="mt-1 rounded px-1 py-1 text-xs text-primary focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={packs.busy} onclick={() => void packs.refresh()}>{packs.blocked ? localeState.messages.instance.detail.refreshResult : localeState.messages.instance.detail.reload}</button></div>{/if}
	{#if packs.message}<p role="status" class="mt-3 text-[12px] text-muted-foreground">{packs.message}</p>{/if}
	{#if packs.selected}
		<div class="mt-3 rounded-lg border border-primary/25 bg-card p-3">
			<div class="flex items-center gap-2"><FileArchive size={16} class="shrink-0 text-primary" aria-hidden="true" /><span class="min-w-0 flex-1 truncate text-[13px]" title={packs.selected.name}>{packs.selected.name}</span><span class="shrink-0 text-[12px] text-muted-foreground">{localPackSize(packs.selected.size, localeState.current)}</span></div>
			<p class="mt-2 text-[12px] leading-5 text-muted-foreground">{localeState.messages.instance.detail.importShaderConfirm}</p>
			<div class="mt-2 flex justify-end gap-1"><button type="button" class={secondary} disabled={packs.busy} onclick={() => packs.selected = null}>{localeState.messages.instance.detail.cancel}</button><button type="button" disabled={locked} onclick={() => void packs.importSelected()} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if packs.activity === 'importing'}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.instance.detail.importing}{:else}{localeState.messages.instance.detail.confirmImport}{/if}</button></div>
		</div>
	{/if}
	{#if !packs.collection && packs.busy}
		<div role="status" class="flex min-h-40 flex-1 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.instance.detail.loadingShaders}</div>
	{:else if packs.collection}
		{#if packs.collection.packs.length === 0}
			<div class="mt-3 flex flex-wrap items-center gap-3 rounded-lg bg-card px-3.5 py-3">
				<span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><Sparkles size={18} strokeWidth={1.5} aria-hidden="true" /></span>
				<div class="min-w-0 flex-1"><p class="text-[13px] font-medium">{localeState.messages.instance.detail.noShaders}</p><p class="mt-1 text-[12px] leading-4 text-muted-foreground">{localeState.messages.instance.shaderEmptyHint}</p></div>
			</div>
		{:else}
			<label class="mt-4 flex h-8 shrink-0 items-center gap-2 rounded-lg bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={14} aria-hidden="true" /><input type="search" bind:value={search} placeholder={localeState.messages.instance.detail.searchShaderName} aria-label={localeState.messages.instance.detail.searchShaders} class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" />{#if search}<button type="button" onclick={() => search = ''} aria-label={localeState.messages.instance.detail.clearSearch} class="rounded p-0.5 focus-visible:outline-2 focus-visible:outline-ring"><X size={12} aria-hidden="true" /></button>{/if}</label>
			<ul aria-label={localeState.messages.instance.detail.instanceShaders} aria-busy={packs.busy} class="resource-list resource-list-icons mt-3 shrink-0">
				{#each visible as pack (pack.sha256)}
					<li class="resource-row rounded-md px-2.5 py-3 transition-colors hover:bg-chrome-hover">
						<div class="flex items-start gap-2.5"><div class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><FileArchive size={17} strokeWidth={1.5} aria-hidden="true" /></div><div class="min-w-0 flex-1"><h2 class="truncate text-[13px] font-medium" title={pack.filename}>{pack.filename}</h2><p class="mt-1.5 text-[12px] text-muted-foreground">{localPackSize(pack.size, localeState.current)} · {localeState.messages.instance.compatibilityUnknown} · {applied ? localeState.messages.instance.detail.added : localeState.messages.instance.detail.needsSync}</p></div><button type="button" class={`${secondary} w-7 px-0 hover:text-destructive`} disabled={locked} aria-label={localeState.messages.instance.removeFile(pack.filename)} title={localeState.messages.instance.removeTitle} onclick={() => { packs.selected = null; packs.removing = pack.sha256; }}><Trash2 size={13} aria-hidden="true" /></button></div>
						{#if packs.removing === pack.sha256}<div class="mt-2 flex flex-wrap items-center justify-end gap-1 rounded-lg bg-chrome-hover px-2 py-2"><p class="mr-auto text-[12px] text-muted-foreground">{localeState.messages.instance.detail.removePackHint}</p><button type="button" class={secondary} disabled={packs.busy} onclick={() => packs.removing = null}>{localeState.messages.instance.detail.cancel}</button><button type="button" disabled={locked} onclick={() => void packs.remove(pack)} class="h-7 rounded-lg px-2 text-xs text-destructive focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{packs.activity === 'removing' ? localeState.messages.instance.detail.removing : localeState.messages.instance.detail.confirmRemove}</button></div>{/if}
					</li>
				{/each}
			</ul>
			{#if visible.length === 0}<p role="status" class="py-6 text-center text-xs text-muted-foreground">{localeState.messages.instance.detail.noShaderMatches}</p>{/if}
		{/if}
	{/if}
</section>
