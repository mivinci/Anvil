<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	import { resolve } from '$app/paths';
	import { getResourceInstallState } from '#lib/resource-install-state.svelte.js';
	const resources = getResourceInstallState();
	let refreshedOperation = $state<number | null>(null);
	const detail = getClientDetailState();
	import { page } from '$app/state';
	import { onDestroy } from 'svelte';
	import { Popover } from 'bits-ui';
	import { Boxes, Check, Copy, FileArchive, Hash, LoaderCircle, Pause, Play, Plus, RefreshCw, Trash2 } from '@lucide/svelte';
	import { listClientResources, importClientMod, changeClientMod, syncClientResources, RuntimeRequestError, type ClientResourceManifest, type ClientResourceEntry } from '../../../../lib/runtime';
	let manifest = $state<ClientResourceManifest | null>(null);
	let busy = $state(false);
	let errorDetail = $state<string | null>(null);
	let errorValue = $state<string | (() => string | null) | null>(null);
	const error = $derived(typeof errorValue === 'function' ? errorValue() : errorValue);
	let uncertain = $state(false);
	let selected = $state<File | null>(null);
	let removing = $state<string | null>(null);
	let search = $state('');
	let copyTarget = $state<string | null>(null);
	let copyOk = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | undefined;
	let input: HTMLInputElement;
	let request: AbortController | null = null;
	const fabric = $derived(detail.client?.loader?.kind === 'fabric');
	const applied = $derived(fabric && manifest !== null && manifest.appliedRevision === manifest.revision);
	const name = $derived(page.params.name ?? '');
	const canBrowse = $derived(fabric && !busy && !uncertain);
	const discoveryHref = $derived(`${resolve('/mods')}?${new URLSearchParams({ instance: name.toLowerCase() })}`);
	const visible = $derived(manifest?.mods.filter((mod) => mod.filename.toLowerCase().includes(search.trim().toLowerCase())) ?? []);
	type ModRelation = { requires: string[]; requiredBy: { name: string; enabled: boolean }[] };
	const relations = $derived.by(() => {
		const map = new Map<string, ModRelation>();
		const mods = manifest?.mods ?? [];
		const idOf = (entry: ClientResourceEntry) => entry.modrinthOrigin?.projectId ?? entry.origin?.projectId;
		const dependenciesOf = (entry: ClientResourceEntry): (string | number)[] => entry.modrinthOrigin ? Object.keys(entry.modrinthOrigin.requiredVersions) : entry.origin ? entry.origin.requiredProjects : [];
		const nameOf = (id: string | number) => {
			const entry = mods.find((candidate) => String(idOf(candidate)) === String(id));
			return entry ? entry.modrinthOrigin?.projectName ?? entry.origin?.projectName ?? entry.filename : localeState.messages.instance.projectFallback(id);
		};
		for (const mod of mods) {
			const own = idOf(mod);
			const requiredBy = own === undefined ? [] : mods
				.filter((other) => other.sha256 !== mod.sha256 && dependenciesOf(other).some((id) => String(id) === String(own)))
				.map((other) => ({ name: other.modrinthOrigin?.projectName ?? other.origin?.projectName ?? other.filename, enabled: other.enabled }));
			map.set(mod.sha256, { requires: dependenciesOf(mod).map(nameOf), requiredBy });
		}
		return map;
	});
	function validate(result: ClientResourceManifest) {
		// Schema 5 adds shaderPacks; this page only reads the mods collection.
		if (!result || ![1, 2, 3, 4, 5].includes(result.schemaVersion) || !Number.isSafeInteger(result.revision) || !Array.isArray(result.mods) || result.mods.length > 1024 || result.mods.some((entry) => typeof entry.filename !== 'string' || !/^[a-f0-9]{64}$/.test(entry.sha256) || !Number.isSafeInteger(entry.size) || typeof entry.enabled !== 'boolean' || !['local', 'curseforge', 'modrinth'].includes(entry.source))) throw new Error(localeState.messages.instance.detail.invalidResourceList);
		return result;
	}
	async function run(action: (signal: AbortSignal) => Promise<ClientResourceManifest>, mutation = false) {
		if (request) return;
		const controller = new AbortController(); request = controller;
		busy = true; errorValue = null;
		try {
			const result = validate(await action(AbortSignal.any([controller.signal, AbortSignal.timeout(60_000)])));
			if (!controller.signal.aborted) { manifest = result; uncertain = false; if (mutation) { selected = null; removing = null; } }
		} catch (reason) {
			if (!controller.signal.aborted) {
				const rejected = reason instanceof RuntimeRequestError && reason.status >= 400 && reason.status < 500;
				if (mutation && !rejected) uncertain = true;
				errorDetail = reason instanceof Error ? reason.message : String(reason);
				errorValue = () => (rejected ? localeState.messages.instance.mutationRejected : mutation ? localeState.messages.instance.mutationUnknown : localeState.messages.instance.detail.resourcesReadFailed);
			}
		} finally { if (request === controller) { request = null; busy = false; } }
	}
	function refresh() { const target = name; void run((signal) => listClientResources(target, signal)); }
	$effect(() => {
		const target = name;
		manifest = null; selected = null; removing = null; search = ''; uncertain = false;
		void run((signal) => listClientResources(target, signal));
		return () => { request?.abort(); request = null; busy = false; };
	});
	$effect(() => {
		const operation = resources.operation;
		if (operation && operation.state !== 'running' && operation.name === name.toLowerCase() && refreshedOperation !== operation.id && !busy) {
			refreshedOperation = operation.id;
			refresh();
		}
	});
	function pick(event: Event) {
		const element = event.currentTarget as HTMLInputElement;
		const file = element.files?.[0]; element.value = '';
		if (!file) return;
		if (!file.name.toLowerCase().endsWith('.jar') || file.size === 0 || file.size > 64 * 1024 * 1024) { errorValue = () => (localeState.messages.instance.detail.invalidJar); return; }
		selected = file; errorValue = null;
	}
	function importFile() {
		if (!manifest || !selected || busy || uncertain) return;
		const target = name, file = selected, revision = manifest.revision;
		void run((signal) => importClientMod(target, file, revision, signal), true);
	}
	function change(mod: ClientResourceEntry, enabled: boolean | null) {
		if (!manifest || busy || uncertain) return;
		const target = name, revision = manifest.revision;
		void run((signal) => changeClientMod(target, mod.sha256, revision, enabled, signal), true);
	}
	function apply() {
		if (!manifest || busy || uncertain || !fabric) return;
		const target = name, revision = manifest.revision;
		void run((signal) => syncClientResources(target, revision, signal), true);
	}
	function size(bytes: number) { return bytes < 1024 * 1024 ? `${new Intl.NumberFormat(localeState.current, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1024)} KiB` : `${new Intl.NumberFormat(localeState.current, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1024 / 1024)} MiB`; }
	async function copyHash(hash: string) {
		if (copyTimer) clearTimeout(copyTimer);
		try { await navigator.clipboard.writeText(hash); copyTarget = hash; copyOk = true; }
		catch { copyTarget = hash; copyOk = false; }
		copyTimer = setTimeout(() => { copyTarget = null; copyTimer = undefined; }, 1500);
	}
	onDestroy(() => { if (copyTimer) clearTimeout(copyTimer); });
</script>

<svelte:head><title>{localeState.messages.common.pageTitle(localeState.messages.nav.mods)}</title></svelte:head>
<section aria-labelledby="client-mods-title" class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	<div class="flex items-center justify-between gap-3"><div class="flex items-baseline gap-2"><h1 id="client-mods-title" class="text-base font-semibold tracking-tight">{localeState.messages.instance.detail.mods}</h1>{#if manifest}<span class="text-xs text-muted-foreground">{localeState.messages.instance.count(manifest.mods.length, 'references')}</span>{/if}</div><div class="flex gap-1.5">{#if fabric && manifest && applied}<button type="button" disabled={busy || uncertain} onclick={apply} title={localeState.messages.instance.detail.syncMods} class="h-7 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">{localeState.messages.instance.detail.syncFiles}</button>{/if}<button type="button" disabled={busy} aria-label={localeState.messages.instance.detail.refreshMods} title={localeState.messages.instance.detail.refresh} onclick={refresh} class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={busy ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button><button type="button" disabled={busy || !manifest || uncertain} onclick={() => input.click()} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">{localeState.messages.instance.detail.localImport}</button><a href={canBrowse ? discoveryHref : undefined} aria-disabled={!canBrowse} tabindex={canBrowse ? 0 : -1} aria-label={localeState.messages.instance.addModsFor(name)} title={fabric ? localeState.messages.instance.browseMods : localeState.messages.instance.detail.fabricRequired} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring aria-disabled:pointer-events-none aria-disabled:opacity-40"><Plus size={13} aria-hidden="true" />{localeState.messages.instance.detail.addMod}</a></div></div>
	<input bind:this={input} type="file" accept=".jar" aria-label={localeState.messages.instance.detail.chooseModJar} onchange={pick} class="hidden" />
	<p class="mt-2 text-xs leading-5 text-muted-foreground">{fabric ? localeState.messages.instance.detail.modsDescription : localeState.messages.instance.detail.vanillaMods}</p>
	{#if error}<p role="alert" class="mt-3 wrap-break-word rounded-lg bg-destructive/5 px-3 py-2 text-xs text-destructive">{error}</p>{/if}
	{#if fabric && manifest && !applied && manifest.mods.some((mod) => mod.enabled)}<div class="mt-3 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"><span class="text-xs text-muted-foreground">{localeState.messages.instance.detail.unappliedMods}</span><button type="button" disabled={busy || uncertain} onclick={apply} class="shrink-0 rounded-md px-2 py-1 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40">{localeState.messages.instance.detail.applyGame}</button></div>{/if}
	{#if error && errorDetail}<details class="mt-2 text-xs text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{errorDetail}</p></details>{/if}
	{#if selected}
		<div class="mt-3 rounded-lg bg-card p-3"><div class="flex items-center gap-2"><FileArchive size={15} class="shrink-0 text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate text-body" title={selected.name}>{selected.name}</span><span class="text-xs text-muted-foreground">{size(selected.size)}</span></div><p class="mt-2 text-xs leading-4 text-muted-foreground">{localeState.messages.instance.detail.importModHint}</p><div class="mt-2 flex justify-end gap-2"><button type="button" disabled={busy} onclick={() => selected = null} class="h-7 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover">{localeState.messages.instance.detail.cancel}</button><button type="button" disabled={busy || uncertain} onclick={importFile} class="h-7 rounded-md bg-primary px-2.5 text-xs text-primary-foreground disabled:opacity-40">{busy ? localeState.messages.instance.detail.importing : localeState.messages.instance.detail.confirmImport}</button></div></div>
	{/if}
	{#if !manifest && busy}<div role="status" class="flex min-h-40 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.instance.detail.loadingResources}</div>
	{:else if manifest}
		{#if manifest.mods.length === 0}<div class="flex min-h-48 flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"><Boxes size={25} strokeWidth={1.5} aria-hidden="true" /><p class="text-xs">{localeState.messages.instance.detail.noMods}</p></div>
		{:else}
			<input type="search" bind:value={search} placeholder={localeState.messages.instance.detail.searchModFilename} aria-label={localeState.messages.instance.detail.searchMods} class="mt-4 h-8 w-full shrink-0 rounded-lg bg-chrome-hover px-2.5 text-xs outline-none focus:ring-1 focus:ring-ring" />
			<ul class="resource-list mt-3" aria-label={localeState.messages.instance.detail.instanceMods}>
				{#each visible as mod (mod.sha256)}
					{@const relation = relations.get(mod.sha256) ?? { requires: [], requiredBy: [] }}
					<li class="resource-row rounded-md px-2.5 py-2.5 transition-colors hover:bg-chrome-hover"><div class="flex items-center gap-2"><div class="min-w-0 flex-1"><p class="truncate text-body" title={mod.filename}>{mod.modrinthOrigin?.projectName ?? mod.origin?.projectName ?? mod.filename}</p><p class="mt-1 text-xs text-muted-foreground">{mod.source === 'modrinth' ? `Modrinth · ${mod.modrinthOrigin?.direct ? localeState.messages.instance.detail.directAdd : localeState.messages.instance.detail.requiredDependency}` : mod.source === 'curseforge' ? `CurseForge · ${mod.origin?.direct ? localeState.messages.instance.detail.directAdd : localeState.messages.instance.detail.requiredDependency}` : localeState.messages.instance.detail.localImport} · {size(mod.size)} · <span class={`font-medium ${mod.enabled ? (applied ? 'text-emerald-600 dark:text-emerald-400' : 'text-amber-600 dark:text-amber-400') : 'text-muted-foreground'}`}>{mod.enabled ? (applied ? localeState.messages.instance.detail.enabled : localeState.messages.instance.detail.notApplied) : localeState.messages.instance.detail.disabled}</span></p></div><button type="button" disabled={busy || uncertain || (!fabric && !mod.enabled)} aria-pressed={mod.enabled} aria-label={`${mod.enabled ? localeState.messages.instance.detail.disable : localeState.messages.instance.detail.enable} ${mod.filename}`} title={mod.enabled ? localeState.messages.instance.detail.disable : localeState.messages.instance.detail.enable} onclick={() => change(mod, !mod.enabled)} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if mod.enabled}<Pause size={13} aria-hidden="true" />{:else}<Play size={13} aria-hidden="true" />{/if}</button><Popover.Root><Popover.Trigger aria-label={localeState.messages.instance.detail.viewIdentity} title={localeState.messages.instance.detail.fileIdentity} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><Hash size={13} aria-hidden="true" /></Popover.Trigger><Popover.Portal to=".app-window"><Popover.Content side="bottom" align="end" sideOffset={6} collisionPadding={8} class="z-30 w-80 max-w-[calc(100vw-32px)] rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-lg outline-none" aria-label={localeState.messages.instance.detail.fileIdentity}><p class="truncate text-xs font-medium" title={mod.filename}>{mod.filename}</p><p class="mt-1 text-xs text-muted-foreground">{mod.source === 'modrinth' ? 'Modrinth' : mod.source === 'curseforge' ? 'CurseForge' : localeState.messages.instance.detail.localImport} · {size(mod.size)}</p><p class="mt-2 text-xs text-muted-foreground">SHA-256</p><p class="mt-1 select-all break-all font-mono text-xs leading-4">{mod.sha256}</p>{#if relation.requires.length || relation.requiredBy.length}<div class="mt-2 space-y-1 border-t border-border pt-2 text-xs leading-5">{#if relation.requires.length}<p><span class="text-muted-foreground">{localeState.messages.instance.detail.dependencies}</span>{relation.requires.join(localeState.current === 'en' ? ', ' : '、')}</p>{/if}{#if relation.requiredBy.length}<p><span class="text-muted-foreground">{localeState.messages.instance.detail.requiredBy}</span>{relation.requiredBy.map((item) => `${item.name}${item.enabled ? '' : localeState.messages.instance.detail.disabledSuffix}`).join(localeState.current === 'en' ? ', ' : '、')}</p>{/if}</div>{/if}<div class="mt-2 flex justify-end"><button type="button" onclick={() => void copyHash(mod.sha256)} class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{#if copyTarget === mod.sha256 && copyOk}<Check size={12} aria-hidden="true" />{:else}<Copy size={12} aria-hidden="true" />{/if}{copyTarget === mod.sha256 ? (copyOk ? localeState.messages.instance.detail.copied : localeState.messages.instance.detail.copyFailed) : localeState.messages.instance.detail.copyHash}</button></div></Popover.Content></Popover.Portal></Popover.Root><button type="button" disabled={busy || uncertain} aria-label={localeState.messages.instance.removeFile(mod.filename)} title={localeState.messages.instance.removeReferenceTitle} onclick={() => removing = mod.sha256} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover hover:text-destructive focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Trash2 size={13} aria-hidden="true" /></button></div>
						{#if removing === mod.sha256}<div class="mt-2 flex flex-wrap items-center justify-end gap-2 rounded-md bg-chrome-hover px-2 py-2 text-xs"><span class="mr-auto text-muted-foreground">{localeState.messages.instance.detail.removeReference}</span><button type="button" disabled={busy} onclick={() => removing = null} class="rounded px-1.5 py-1 text-muted-foreground">{localeState.messages.instance.detail.cancel}</button><button type="button" disabled={busy || uncertain} onclick={() => change(mod, null)} class="rounded px-1.5 py-1 text-destructive disabled:opacity-40">{localeState.messages.instance.detail.confirmRemove}</button></div>{/if}
					</li>
				{/each}
			</ul>
			{#if visible.length === 0}<p class="py-5 text-center text-xs text-muted-foreground">{localeState.messages.instance.detail.noModMatches}</p>{/if}
		{/if}
	{/if}
</section>
