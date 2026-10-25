<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Dialog } from 'bits-ui';
	import { ChevronLeft, Download, LoaderCircle, RefreshCw, Search, X } from '@lucide/svelte';
	import ModProjectIcon from '../mods/mod-project-icon.svelte';
	import { formatBytes } from '../../download-progress';
	import { formatModDownloads, formatModUpdated } from '../../mod-project-stats';
	import { downloadShaderPack, getShaderProject, listShaderVersions, searchShaderProjects, type ModProjectSummary, type ModrinthFile } from '../../runtime';

	let {
		name,
		gameVersion,
		revision,
		onImported,
	}: {
		name: string;
		gameVersion: string | null;
		revision: number | null;
		onImported: () => void;
	} = $props();
	let open = $state(false);
	let query = $state('');
	let searching = $state(false);
	let loadingVersions = $state(false);
	let installing = $state<string | null>(null);
	let error = $state<string | null>(null);
	let projects = $state<ModProjectSummary[]>([]);
	let nextIndex = $state<number | null>(null);
	let selected = $state<ModProjectSummary | null>(null);
	let versions = $state<ModrinthFile[]>([]);
	let request: AbortController | null = null;
	const button = 'inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40';

	function abort() {
		request?.abort();
		request = null;
		searching = false;
		loadingVersions = false;
	}
	function begin() {
		abort();
		const controller = new AbortController();
		request = controller;
		return controller;
	}
	function show() {
		if (!gameVersion || revision === null) return;
		open = true;
		error = null;
		if (projects.length === 0) void search(0);
	}
	async function search(index: number) {
		if (!gameVersion) return;
		const controller = begin();
		searching = true;
		error = null;
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
		if (!gameVersion) return;
		const controller = begin();
		selected = project;
		versions = [];
		loadingVersions = true;
		error = null;
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
	}
	async function install(file: ModrinthFile) {
		if (!selected || revision === null || installing) return;
		installing = file.id;
		error = null;
		try {
			await downloadShaderPack(name, String(selected.id), file.id, revision, AbortSignal.timeout(120_000));
			onImported();
			open = false;
			selected = null;
			versions = [];
		} catch (reason) {
			error = reason instanceof Error ? reason.message : '下载未确认，请刷新后重试。';
		} finally {
			installing = null;
		}
	}
	onDestroy(abort);
</script>

<button type="button" disabled={!gameVersion || revision === null} onclick={show} title={gameVersion ? '从 Modrinth 浏览并下载光影' : '实例信息尚未读取'} class={button}>
	<Download size={13} aria-hidden="true" />在线获取
</button>

<Dialog.Root bind:open onOpenChange={(value) => { if (!value) abort(); }}>
	<Dialog.Portal to=".app-window">
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
		<Dialog.Content class="fixed top-1/2 left-1/2 z-50 flex h-[min(620px,calc(100vh-48px))] w-[min(720px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-xl bg-popover p-4 text-popover-foreground shadow-xl outline-none">
			<div class="flex shrink-0 items-start justify-between gap-3">
				<div><Dialog.Title class="text-sm font-semibold">在线获取光影</Dialog.Title><Dialog.Description class="mt-1 text-[11px] leading-4 text-muted-foreground">Minecraft {gameVersion ?? '未知版本'} · 仅显示 Iris 可用版本，下载后自动校验并添加到此实例。</Dialog.Description></div>
				<button type="button" aria-label="关闭" title="关闭" onclick={() => { open = false; abort(); }} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button>
			</div>

			{#if !selected}
				<form class="mt-3 flex shrink-0 gap-2" onsubmit={(event) => { event.preventDefault(); void search(0); }}>
					<input type="search" bind:value={query} aria-label="搜索光影" placeholder="搜索 Modrinth 光影" class="h-9 min-w-0 flex-1 rounded-lg bg-chrome-hover px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring" />
					<button type="submit" disabled={searching} class="grid size-9 shrink-0 place-items-center rounded-lg bg-chrome-hover text-muted-foreground hover:bg-chrome-selected disabled:opacity-40" aria-label="搜索" title="搜索">{#if searching}<LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Search size={15} aria-hidden="true" />{/if}</button>
				</form>
				{#if error}<p role="alert" class="mt-2 shrink-0 text-xs text-destructive">{error}</p>{/if}
				<div class="mt-3 min-h-0 flex-1 overflow-y-auto overscroll-contain">
					{#if searching && projects.length === 0}
						<p role="status" class="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取光影项目…</p>
					{:else if projects.length === 0}
						<p class="py-16 text-center text-xs text-muted-foreground">没有找到匹配的光影</p>
					{:else}
						<ul class="divide-y divide-border/70 rounded-lg bg-card px-3" aria-label="光影项目">
							{#each projects as project (project.id)}
								<li><button type="button" onclick={() => void select(project)} class="flex w-full items-center gap-3 py-2.5 text-left focus-visible:outline-2 focus-visible:outline-ring"><ModProjectIcon {project} source="modrinth" /><span class="min-w-0 flex-1"><span class="block truncate text-[13px] font-medium" title={project.name}>{project.name}</span><span class="mt-0.5 line-clamp-1 text-[11px] text-muted-foreground" title={project.summary}>{project.summary}</span></span><span class="shrink-0 text-right text-[10px] leading-4 text-muted-foreground">{formatModDownloads(project.downloadCount) ?? ''}<br />{formatModUpdated(project.dateModified) ?? ''}</span></button></li>
							{/each}
						</ul>
						{#if nextIndex !== null}<button type="button" disabled={searching} class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" onclick={() => void search(nextIndex!)}>{#if searching}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />加载中…{:else}加载更多{/if}</button>{/if}
					{/if}
				</div>
			{:else}
				<div class="mt-3 flex shrink-0 items-center gap-2">
					<button type="button" class={button} onclick={back}><ChevronLeft size={13} aria-hidden="true" />返回结果</button>
				</div>
				<div class="mt-2 flex shrink-0 items-start gap-3 rounded-lg bg-card px-3 py-2.5">
					<ModProjectIcon project={selected} source="modrinth" large />
					<div class="min-w-0 flex-1"><h3 class="break-words text-[13px] font-medium">{selected.name}</h3><p class="mt-1 line-clamp-2 text-[11px] leading-4 text-muted-foreground">{selected.summary}</p></div>
				</div>
				{#if error}<p role="alert" class="mt-2 shrink-0 text-xs text-destructive">{error}</p>{/if}
				<div class="mt-3 min-h-0 flex-1 overflow-y-auto overscroll-contain">
					{#if loadingVersions}
						<p role="status" class="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取兼容版本…</p>
					{:else if versions.length === 0}
						<p class="py-16 text-center text-xs text-muted-foreground">没有 Minecraft {gameVersion} 可用的 Iris 光影版本</p>
					{:else}
						<ul class="divide-y divide-border/70 rounded-lg bg-card px-3" aria-label="光影版本">
							{#each versions as file (file.id)}
								<li class="flex items-center gap-3 py-2.5">
									<div class="min-w-0 flex-1"><p class="truncate text-[13px] font-medium" title={file.displayName}>{file.displayName}</p><p class="mt-0.5 text-[11px] text-muted-foreground">{file.fileDate.slice(0, 10)} · {formatBytes(file.fileLength)}</p></div>
									<button type="button" disabled={installing !== null} onclick={() => void install(file)} class="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if installing === file.id}<LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />下载中…{:else}<Download size={12} aria-hidden="true" />下载并导入{/if}</button>
								</li>
							{/each}
						</ul>
						<button type="button" class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" disabled={loadingVersions} onclick={() => { if (selected) void select(selected); }}><RefreshCw size={12} aria-hidden="true" />刷新版本</button>
					{/if}
				</div>
			{/if}
			<p class="mt-3 shrink-0 text-[10px] leading-4 text-muted-foreground">下载使用现有 q2q-http 校验与实例事务；完成后仍需在游戏内选择光影。暂不支持取消或 CurseForge 光影。</p>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
