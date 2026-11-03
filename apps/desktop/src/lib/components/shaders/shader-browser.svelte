<script lang="ts">
	import { runtimeErrorMessage, requestErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.js';
	import { onMount, untrack } from 'svelte';
	import { Check, Download, LoaderCircle, RefreshCw, Search } from '@lucide/svelte';
	import ModProjectIcon from '../mods/mod-project-icon.svelte';
	import { formatBytes as downloadBytes } from '../../download-progress';
	function formatBytes(value: number) { return downloadBytes(value, localeState.current); }
	import { formatModDownloads, formatModUpdated } from '../../mod-project-stats';
	import { createShaderInstallPlan, getShaderProject, listClientShaderPacks, listShaderVersions, searchShaderProjects, type ModProjectSummary, type ModrinthFile, type ShaderInstallPlan } from '../../runtime';
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';

	let {
		name,
		gameVersion,
		onImported,
	}: {
		name: string;
		gameVersion: string;
		onImported?: () => void;
	} = $props();
	let query = $state('');
	let searching = $state(false);
	let loadingVersions = $state(false);
	let planning = $state(false);
	let plan = $state<ShaderInstallPlan | null>(null);
	let handled = $state<number | null>(null);
	let loadingRevision = $state(true);
	let revision = $state<number | null>(null);
	let errorDetail = $state<string | null>(null);
	let errorValue = $state<string | (() => string | null) | null>(null);
	const error = $derived(typeof errorValue === 'function' ? errorValue() : errorValue);
	let messageValue = $state<string | (() => string | null) | null>(null);
	const message = $derived(typeof messageValue === 'function' ? messageValue() : messageValue);
	let projects = $state<ModProjectSummary[]>([]);
	let nextIndex = $state<number | null>(null);
	let selected = $state<ModProjectSummary | null>(null);
	let versions = $state<ModrinthFile[]>([]);
	let pending = $state<ModrinthFile | null>(null);
	let request: AbortController | null = null;
	const resources = getResourceInstallState();
	const operation = $derived(plan && resources.operation?.id === plan.planId ? resources.operation : null);
	const busy = $derived(planning || resources.starting || resources.active);
	const percent = $derived(operation && operation.progress.totalBytes > 0 ? Math.min(100, Math.round(operation.progress.receivedBytes / operation.progress.totalBytes * 100)) : null);
	const projectId = $derived(page.url.searchParams.get('project'));
	const button = 'inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	function abort() {
		request?.abort();
		request = null;
		searching = false;
		loadingVersions = false;
		planning = false;
	}
	function begin() {
		abort();
		const controller = new AbortController();
		request = controller;
		return controller;
	}
	async function loadRevision() {
		abort();
		const controller = begin();
		loadingRevision = true;
		try {
			const snapshot = await listClientShaderPacks(name, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (request === controller && !controller.signal.aborted) revision = snapshot.revision;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) { errorDetail = reason instanceof Error ? reason.message : String(reason); errorValue = () => requestErrorMessage(reason, localeState.current, (localeState.messages.mods.discovery.shaderListFailed)); }
		} finally {
			if (request === controller) { request = null; loadingRevision = false; }
		}
	}
	async function search(index: number) {
		const controller = begin();
		searching = true;
		pending = null;
		plan = null;
		errorValue = null; errorDetail = null;
		messageValue = null;
		try {
			const page = await searchShaderProjects(query.trim(), index, gameVersion, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (request !== controller || controller.signal.aborted) return;
			projects = index === 0 ? page.items : [...projects, ...page.items.filter((item) => !projects.some((existing) => existing.id === item.id))];
			nextIndex = page.nextIndex;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) { errorDetail = reason instanceof Error ? reason.message : String(reason); errorValue = () => requestErrorMessage(reason, localeState.current, (localeState.messages.mods.discovery.shaderProjectsFailed)); }
		} finally {
			if (request === controller) { request = null; searching = false; }
		}
	}
	async function openProject(project: ModProjectSummary) {
		if (busy) return;
		const next = new URL(page.url.href);
		next.searchParams.set('project', String(project.id));
		await goto(`${next.pathname}${next.search}`, { reset: false });
	}
	async function loadProject(id: string) {
		const controller = begin();
		selected = projects.find((project) => String(project.id) === id) ?? selected;
		versions = [];
		pending = null;
		plan = null;
		loadingVersions = true;
		errorValue = null; errorDetail = null;
		messageValue = null;
		try {
			const [details, value] = await Promise.all([
				getShaderProject(id, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)])),
				listShaderVersions(id, gameVersion, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)])),
			]);
			if (request !== controller || controller.signal.aborted) return;
			selected = details.project;
			versions = value.items;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) { errorDetail = reason instanceof Error ? reason.message : String(reason); errorValue = () => requestErrorMessage(reason, localeState.current, (localeState.messages.mods.discovery.shaderVersionsFailed)); }
		} finally {
			if (request === controller) { request = null; loadingVersions = false; }
		}
	}
	$effect(() => {
		const id = projectId;
		untrack(() => {
			if (!id) {
				abort();
				selected = null;
				versions = [];
				pending = null;
				plan = null;
				errorValue = null; errorDetail = null;
				messageValue = null;
				return;
			}
			void loadProject(id);
		});
	});
	function releaseLabel(type: number) {
		return type === 1 ? 'Release' : type === 2 ? 'Beta' : 'Alpha';
	}
	async function choose(file: ModrinthFile) {
		if (!selected || revision === null || planning || busy) return;
		const controller = begin();
		pending = file;
		plan = null;
		handled = null;
		planning = true;
		errorValue = null; errorDetail = null;
		messageValue = null;
		try {
			const value = await createShaderInstallPlan(name, String(selected.id), file.id, revision, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (request !== controller || controller.signal.aborted) return;
			plan = value;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) { errorDetail = reason instanceof Error ? reason.message : String(reason); errorValue = () => requestErrorMessage(reason, localeState.current, (localeState.messages.mods.discovery.shaderPlanFailed)); }
		} finally {
			if (request === controller) { request = null; planning = false; }
		}
	}
	async function install() {
		if (!plan || !resources.canSubmit(plan.planId)) return;
		errorValue = null; errorDetail = null;
		messageValue = null;
		if (!(await resources.start(name, plan.planId))) { errorDetail = resources.errorDetail; errorValue = () => resources.error; }
	}
	async function cancelOperation() {
		await resources.cancel();
		if (resources.error) { errorDetail = resources.errorDetail; errorValue = () => resources.error; }
	}
	function resetPending() {
		abort();
		pending = null;
		plan = null;
		errorValue = null; errorDetail = null;
		messageValue = null;
	}
	function downloads() {
		document.querySelector<HTMLButtonElement>('[data-download-tasks-trigger]')?.click();
	}
	$effect(() => {
		const value = operation;
		if (!value || !plan || handled === value.id) return;
		if (value.state === 'completed') {
			handled = value.id;
			messageValue = () => (localeState.messages.mods.discovery.shaderAdded);
			pending = null;
			plan = null;
			void loadRevision();
			onImported?.();
		} else if (value.state === 'failed' || value.state === 'cancelled') {
			handled = value.id;
			errorDetail = value.error; const cancelled = value.state === 'cancelled'; errorValue = () => runtimeErrorMessage(value.errorCode, localeState.current, cancelled ? localeState.messages.mods.discovery.downloadCancelled : localeState.messages.mods.discovery.downloadFailed);
			plan = null;
		}
	});
	onMount(() => { void (async () => { await loadRevision(); await search(0); })(); });
</script>

<div class="flex min-h-0 flex-1 flex-col">
	<form class="flex shrink-0 gap-2" onsubmit={(event) => { event.preventDefault(); void search(0); }}>
		<input type="search" bind:value={query} aria-label={localeState.messages.mods.discovery.searchShaders} placeholder={localeState.messages.mods.discovery.searchModrinthShaders} class="h-9 min-w-0 flex-1 rounded-lg bg-chrome-hover px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring" />
		<button type="submit" disabled={searching} class="grid size-9 shrink-0 place-items-center rounded-lg bg-chrome-hover text-muted-foreground hover:bg-chrome-selected disabled:opacity-40" aria-label={localeState.messages.mods.discovery.search} title={localeState.messages.mods.discovery.search}>{#if searching}<LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Search size={15} aria-hidden="true" />{/if}</button>
	</form>
	{#if error}<p role="alert" class="mt-2 shrink-0 text-xs text-destructive">{error}</p>{/if}
	{#if errorDetail}<details class="mt-2 text-[12px] text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{errorDetail}</p></details>{/if}
	{#if message}<p role="status" class="mt-2 shrink-0 text-xs text-emerald-600 dark:text-emerald-400">{message}</p>{/if}
	{#if loadingRevision}<p role="status" class="mt-2 shrink-0 text-[12px] text-muted-foreground">{localeState.messages.mods.discovery.loadingShaderList}</p>{:else if revision === null}<p class="mt-2 shrink-0 text-[12px] text-muted-foreground">{localeState.messages.mods.discovery.revisionUnknown}</p>{/if}
	<div class="mt-3 min-h-0 flex-1 overflow-y-auto overscroll-contain">
		{#if !selected}
			{#if searching && projects.length === 0}
				<p role="status" class="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.loadingShaderProjects}</p>
			{:else if projects.length === 0}
				<p class="py-16 text-center text-xs text-muted-foreground">{localeState.messages.mods.discovery.noShaders}</p>
			{:else}
				<ul class="resource-list resource-list-icons" aria-label={localeState.messages.mods.discovery.shaderProjects}>
					{#each projects as project (project.id)}
						<li class="resource-row"><button type="button" onclick={() => void openProject(project)} class="flex w-full items-center gap-3 rounded-md px-2.5 py-2.5 text-left transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"><ModProjectIcon {project} source="modrinth" /><span class="min-w-0 flex-1"><span class="block truncate text-[13px] font-medium" title={project.name}>{project.name}</span><span class="mt-0.5 line-clamp-1 text-[12px] text-muted-foreground" title={project.summary}>{project.summary}</span></span><span class="shrink-0 text-right text-[12px] leading-4 text-muted-foreground">{formatModDownloads(project.downloadCount, localeState.current) ?? ''}<br />{formatModUpdated(project.updatedAt ?? project.dateModified, localeState.current) ?? ''}</span></button></li>
					{/each}
				</ul>
				{#if nextIndex !== null}<button type="button" disabled={searching} class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" onclick={() => void search(nextIndex!)}>{#if searching}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.loading}{:else}{localeState.messages.mods.discovery.loadMore}{/if}</button>{/if}
			{/if}
		{:else}
			<div class="mt-2 flex items-start gap-3 px-2.5 pt-1 pb-3">
				<ModProjectIcon project={selected} source="modrinth" large />
				<div class="min-w-0 flex-1"><h2 class="break-words text-[13px] font-medium">{selected.name}</h2><p class="mt-1 line-clamp-2 text-[12px] leading-4 text-muted-foreground">{selected.summary}</p></div>
			</div>
			{#if loadingVersions}
				<p role="status" class="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.loadingVersions}</p>
			{:else if versions.length === 0}
				<p class="py-16 text-center text-xs text-muted-foreground">{localeState.messages.mods.noShaderVersions(gameVersion)}</p>
			{:else}
				<div class="mx-2.5 mt-3 flex items-center justify-between gap-3 border-b border-border/60 pb-2"><h3 class="text-[13px] font-medium">{localeState.messages.mods.discovery.compatibleVersions}</h3><span class="text-[12px] text-muted-foreground">Minecraft {gameVersion} · Iris</span></div>
				<div class="resource-list mt-1" aria-label={localeState.messages.mods.discovery.chooseVersion}>
					{#each versions as file (file.id)}
						<button type="button" aria-pressed={pending?.id === file.id} disabled={planning || resources.starting || resources.active} onclick={() => void choose(file)} class={`resource-row flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring disabled:opacity-40 ${pending?.id === file.id ? 'bg-chrome-selected' : 'hover:bg-chrome-hover'}`}>
							<span class="min-w-0 flex-1"><span class="block truncate text-xs font-medium" title={file.displayName}>{file.displayName}</span><span class="mt-1 block truncate text-[12px] text-muted-foreground" title={file.fileName}>{formatModUpdated(file.fileDate, localeState.current) ?? file.fileDate} · {releaseLabel(file.releaseType)} · {formatBytes(file.fileLength)}</span></span>
							{#if pending?.id === file.id}<Check size={14} class="shrink-0 text-primary" aria-hidden="true" />{/if}
						</button>
					{/each}
				</div>
				<button type="button" class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" disabled={loadingVersions} onclick={() => { if (projectId) void loadProject(projectId); }}><RefreshCw size={12} aria-hidden="true" />{localeState.messages.mods.discovery.refreshVersions}</button>
			{/if}
		{/if}
	</div>
	{#if pending}
		<section aria-label={localeState.messages.mods.discovery.confirmShader} class="mt-3 shrink-0 rounded-lg bg-card px-3 py-2.5">
			<div class="flex flex-wrap items-center gap-x-3 gap-y-2">
				<div class="min-w-0 flex-1 text-xs">
					{#if planning}
						<p role="status" class="flex items-center gap-2 text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.preparingDownload}</p>
					{:else if operation?.state === 'running'}
						<p role="status" class="flex items-center gap-2 text-muted-foreground">{#if operation.cancelRequested}{localeState.messages.mods.discovery.cancellingLong}{:else if operation.progress.stage === 'publishing'}{localeState.messages.mods.discovery.applyingShaders}{:else if operation.progress.stage === 'downloading'}{localeState.messages.mods.downloadingShaders(percent === null ? null : new Intl.NumberFormat(localeState.current, {style: 'percent'}).format(percent / 100))}{:else}{localeState.messages.mods.discovery.preparingShaders}{/if}</p>
						<div class="mt-1.5 h-1 overflow-hidden rounded-full bg-chrome-hover"><div class="h-full rounded-full bg-primary transition-[width] duration-300" style:width={`${percent ?? 0}%`}></div></div>
					{:else if error}
						<p role="alert" class="break-words text-destructive">{error}</p>
					{:else if plan}
						<p class="truncate" title={`${plan.projectName} · ${plan.versionName} · ${formatBytes(plan.fileSize)}`}><span class="text-muted-foreground">{localeState.messages.mods.discovery.installTo}</span> {name} <span class="text-muted-foreground">· {plan.projectName} · {plan.versionName} · {formatBytes(plan.fileSize)}</span></p>
						{#if resources.active || resources.starting}<p class="mt-0.5 text-[12px] text-muted-foreground">{localeState.messages.mods.discovery.waitOtherTask}</p>{/if}
					{:else}
						<p class="truncate text-muted-foreground">{localeState.messages.mods.selectedShader(pending.displayName)}</p>
					{/if}
				</div>
				{#if planning}
					<span class="text-[12px] text-muted-foreground">{localeState.messages.mods.discovery.pleaseWait}</span>
				{:else if operation?.state === 'running'}
					<button type="button" class={button} onclick={downloads}>{localeState.messages.mods.discovery.viewTasks}</button>
					<button type="button" class={button} disabled={resources.cancelling || operation.cancelRequested} onclick={() => void cancelOperation()}>{resources.cancelling || operation.cancelRequested ? localeState.messages.mods.discovery.cancelling : localeState.messages.mods.discovery.cancel}</button>
				{:else if error}
					<button type="button" class={button} onclick={resetPending}>{localeState.messages.mods.discovery.cancel}</button>
					<button type="button" class={button} disabled={!pending || revision === null} onclick={() => pending && void choose(pending)}><RefreshCw size={13} aria-hidden="true" />{localeState.messages.mods.discovery.regeneratePlan}</button>
				{:else if plan}
					<button type="button" class={button} onclick={resetPending}>{localeState.messages.mods.discovery.cancel}</button>
					<button type="button" disabled={!resources.canSubmit(plan.planId)} onclick={() => void install()} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if resources.starting}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.submitting}{:else}<Download size={13} aria-hidden="true" />{localeState.messages.mods.discovery.confirmDownload}{/if}</button>
				{/if}
			</div>
		</section>
	{/if}

</div>
