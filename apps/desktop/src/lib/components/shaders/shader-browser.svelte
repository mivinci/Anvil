<script lang="ts">
	import { onMount } from 'svelte';
	import { ChevronLeft, Download, LoaderCircle, RefreshCw, Search } from '@lucide/svelte';
	import ModProjectIcon from '../mods/mod-project-icon.svelte';
	import { formatBytes } from '../../download-progress';
	import { formatModDownloads, formatModUpdated } from '../../mod-project-stats';
	import { downloadShaderPack, getShaderProject, listClientShaderPacks, listShaderVersions, searchShaderProjects, type ModProjectSummary, type ModrinthFile } from '../../runtime';

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
	let installing = $state<string | null>(null);
	let loadingRevision = $state(true);
	let revision = $state<number | null>(null);
	let error = $state<string | null>(null);
	let message = $state<string | null>(null);
	let projects = $state<ModProjectSummary[]>([]);
	let nextIndex = $state<number | null>(null);
	let selected = $state<ModProjectSummary | null>(null);
	let versions = $state<ModrinthFile[]>([]);
	let request: AbortController | null = null;
	const button = 'inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	function abort() {
		request?.abort();
		request = null;
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
			if (request === controller && !controller.signal.aborted) error = reason instanceof Error ? reason.message : '无法读取实例光影清单。';
		} finally {
			if (request === controller) { request = null; loadingRevision = false; }
		}
	}
	async function search(index: number) {
		const controller = begin();
		searching = true;
		error = null;
		message = null;
		try {
			const page = await searchShaderProjects(query.trim(), index, gameVersion, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (request !== controller || controller.signal.aborted) return;
			projects = index === 0 ? page.items : [...projects, ...page.items.filter((item) => !projects.some((existing) => existing.id === item.id))];
			nextIndex = page.nextIndex;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) error = reason instanceof Error ? reason.message : '无法读取光影项目。';
		} finally {
			if (request === controller) { request = null; searching = false; }
		}
	}
	async function select(project: ModProjectSummary) {
		const controller = begin();
		selected = project;
		versions = [];
		loadingVersions = true;
		error = null;
		message = null;
		try {
			const [details, page] = await Promise.all([
				getShaderProject(String(project.id), AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)])),
				listShaderVersions(String(project.id), gameVersion, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)])),
			]);
			if (request !== controller || controller.signal.aborted) return;
			selected = details.project;
			versions = page.items;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) error = reason instanceof Error ? reason.message : '无法读取光影版本。';
		} finally {
			if (request === controller) { request = null; loadingVersions = false; }
		}
	}
	function back() {
		abort();
		selected = null;
		versions = [];
		error = null;
		message = null;
	}
	async function install(file: ModrinthFile) {
		if (!selected || revision === null || installing) return;
		installing = file.id;
		error = null;
		message = null;
		try {
			await downloadShaderPack(name, String(selected.id), file.id, revision, AbortSignal.timeout(120_000));
			message = '已添加到此实例，可在游戏内选择光影。';
			await loadRevision();
			onImported?.();
		} catch (reason) {
			error = reason instanceof Error ? reason.message : '下载未确认，请刷新后重试。';
		} finally {
			installing = null;
		}
	}
	onMount(() => { void (async () => { await loadRevision(); await search(0); })(); });
</script>

<div class="flex min-h-0 flex-1 flex-col">
	<form class="flex shrink-0 gap-2" onsubmit={(event) => { event.preventDefault(); void search(0); }}>
		<input type="search" bind:value={query} aria-label="搜索光影" placeholder="搜索 Modrinth 光影" class="h-9 min-w-0 flex-1 rounded-lg bg-chrome-hover px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring" />
		<button type="submit" disabled={searching} class="grid size-9 shrink-0 place-items-center rounded-lg bg-chrome-hover text-muted-foreground hover:bg-chrome-selected disabled:opacity-40" aria-label="搜索" title="搜索">{#if searching}<LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Search size={15} aria-hidden="true" />{/if}</button>
	</form>
	{#if error}<p role="alert" class="mt-2 shrink-0 text-xs text-destructive">{error}</p>{/if}
	{#if message}<p role="status" class="mt-2 shrink-0 text-xs text-emerald-600 dark:text-emerald-400">{message}</p>{/if}
	{#if loadingRevision}<p role="status" class="mt-2 shrink-0 text-[11px] text-muted-foreground">正在读取实例光影清单…</p>{:else if revision === null}<p class="mt-2 shrink-0 text-[11px] text-muted-foreground">无法确认实例清单，下载按钮暂不可用。</p>{/if}
	<div class="mt-3 min-h-0 flex-1 overflow-y-auto overscroll-contain">
		{#if !selected}
			{#if searching && projects.length === 0}
				<p role="status" class="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取光影项目…</p>
			{:else if projects.length === 0}
				<p class="py-16 text-center text-xs text-muted-foreground">没有找到匹配的光影</p>
			{:else}
				<ul class="divide-y divide-border/70 rounded-lg bg-card px-3" aria-label="光影项目">
					{#each projects as project (project.id)}
						<li><button type="button" onclick={() => void select(project)} class="flex w-full items-center gap-3 py-2.5 text-left focus-visible:outline-2 focus-visible:outline-ring"><ModProjectIcon {project} source="modrinth" /><span class="min-w-0 flex-1"><span class="block truncate text-[13px] font-medium" title={project.name}>{project.name}</span><span class="mt-0.5 line-clamp-1 text-[11px] text-muted-foreground" title={project.summary}>{project.summary}</span></span><span class="shrink-0 text-right text-[10px] leading-4 text-muted-foreground">{formatModDownloads(project.downloadCount) ?? ''}<br />{formatModUpdated(project.updatedAt ?? project.dateModified) ?? ''}</span></button></li>
					{/each}
				</ul>
				{#if nextIndex !== null}<button type="button" disabled={searching} class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" onclick={() => void search(nextIndex!)}>{#if searching}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />加载中…{:else}加载更多{/if}</button>{/if}
			{/if}
		{:else}
			<button type="button" class={button} onclick={back}><ChevronLeft size={13} aria-hidden="true" />返回结果</button>
			<div class="mt-2 flex items-start gap-3 rounded-lg bg-card px-3 py-2.5">
				<ModProjectIcon project={selected} source="modrinth" large />
				<div class="min-w-0 flex-1"><h2 class="break-words text-[13px] font-medium">{selected.name}</h2><p class="mt-1 line-clamp-2 text-[11px] leading-4 text-muted-foreground">{selected.summary}</p></div>
			</div>
			{#if loadingVersions}
				<p role="status" class="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取兼容版本…</p>
			{:else if versions.length === 0}
				<p class="py-16 text-center text-xs text-muted-foreground">没有 Minecraft {gameVersion} 可用的 Iris 光影版本</p>
			{:else}
				<ul class="mt-3 divide-y divide-border/70 rounded-lg bg-card px-3" aria-label="光影版本">
					{#each versions as file (file.id)}
						<li class="flex items-center gap-3 py-2.5">
							<div class="min-w-0 flex-1"><p class="truncate text-[13px] font-medium" title={file.displayName}>{file.displayName}</p><p class="mt-0.5 text-[11px] text-muted-foreground">{file.fileDate.slice(0, 10)} · {formatBytes(file.fileLength)}</p></div>
							<button type="button" disabled={installing !== null || revision === null} onclick={() => void install(file)} class="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if installing === file.id}<LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />下载中…{:else}<Download size={12} aria-hidden="true" />下载并导入{/if}</button>
						</li>
					{/each}
				</ul>
				<button type="button" class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" disabled={loadingVersions} onclick={() => { if (selected) void select(selected); }}><RefreshCw size={12} aria-hidden="true" />刷新版本</button>
			{/if}
		{/if}
	</div>
</div>
