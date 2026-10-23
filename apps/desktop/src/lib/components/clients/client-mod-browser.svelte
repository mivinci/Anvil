<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { isTauri } from '@tauri-apps/api/core';
	import { shell } from '../../shell';
	import { ArrowLeft, ChevronLeft, ChevronRight, LoaderCircle, Search, X, Package, ExternalLink } from '@lucide/svelte';
	import SelectableList from '../ui/selectable-list.svelte';
	import SelectableRow from '../ui/selectable-row.svelte';
	import {
		getCurseForgeConfiguration, searchCurseForgeProjects, listCurseForgeFiles, createCurseForgeInstallPlan,
		type CurseForgeConfiguration, type CurseForgeProject, type CurseForgeFile, type CurseForgePage, type CurseForgeInstallPlan,
	} from '../../runtime';
	let { name, gameVersion, onClose }: { name: string; gameVersion: string; onClose: () => void } = $props();
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';
	const resources = getResourceInstallState();
	let planExpired = $state(false);
	let deadline = $state(0);
	let config = $state<CurseForgeConfiguration | null>(null);
	let projects = $state<CurseForgePage<CurseForgeProject> | null>(null);
	let project = $state<CurseForgeProject | null>(null);
	let files = $state<CurseForgePage<CurseForgeFile> | null>(null);
	let selected = $state<CurseForgeFile | null>(null);
	let preview = $state<CurseForgeInstallPlan | null>(null);
	let query = $state('');
	let submitted = $state('');
	let projectIndex = $state(0);
	let fileIndex = $state(0);
	let projectHistory = $state<number[]>([]);
	let fileHistory = $state<number[]>([]);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let request: AbortController | null = null;
	let retry: (() => void) | null = null;
	const button = 'inline-flex h-7 items-center justify-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	async function run(action: (signal: AbortSignal) => Promise<void>, again: () => void) {
		request?.abort();
		const controller = new AbortController(); request = controller;
		busy = true; error = null; retry = again;
		try { await action(AbortSignal.any([controller.signal, AbortSignal.timeout(55_000)])); }
		catch (reason) { if (!controller.signal.aborted) error = reason instanceof Error ? reason.message : '暂时无法读取 CurseForge，请重试。'; }
		finally { if (request === controller) { request = null; busy = false; } }
	}
	function search(index = 0, text = query.trim()) {
		if (index === 0) projectHistory = [];
		project = null; selected = null; preview = null; files = null; projects = null;
		const target = name;
		void run(async (signal) => {
			const result = await searchCurseForgeProjects(target, text, index, signal);
			signal.throwIfAborted(); projects = result; submitted = text; projectIndex = index;
		}, () => search(index, text));
	}
	function versions(item: CurseForgeProject, index = 0) {
		if (index === 0) fileHistory = [];
		project = item; selected = null; preview = null; files = null;
		const target = name;
		void run(async (signal) => {
			const result = await listCurseForgeFiles(target, item.id, index, signal);
			signal.throwIfAborted(); files = result; fileIndex = index;
		}, () => versions(item, index));
	}
	function plan(file: CurseForgeFile) {
		if (!project) return;
		selected = file; preview = null;
		const target = name, id = project.id;
		void run(async (signal) => {
			const result = await createCurseForgeInstallPlan(target, id, file.id, signal);
			signal.throwIfAborted(); preview = result; deadline = Date.now() + result.expiresInMs; planExpired = false;
			if (resources.uncertain && !resources.intent) void resources.sync();
		}, () => plan(file));
	}
	function initialize(target: string) {
		config = null; projects = null; project = null; files = null; selected = null; preview = null;
		query = ''; submitted = ''; projectIndex = 0; fileIndex = 0; projectHistory = []; fileHistory = [];
		void run(async (signal) => {
			const result = await getCurseForgeConfiguration(signal);
			signal.throwIfAborted(); config = result;
			if (result.configured) {
				const result = await searchCurseForgeProjects(target, '', 0, signal);
				signal.throwIfAborted(); projects = result; projectIndex = 0;
			}
		}, () => initialize(target));
	}
	$effect(() => {
		const target = name;
		initialize(target);
		return () => { request?.abort(); request = null; };
	});
	$effect(() => {
		if (!preview) return;
		const timer = setTimeout(() => { planExpired = true; }, Math.max(0, deadline - Date.now()));
		return () => clearTimeout(timer);
	});
	async function install() {
		if (!preview || busy || planExpired || Date.now() >= deadline) { planExpired = true; return; }
		if (await resources.start(name, preview.planId)) onClose();
	}
	function back() {
		request?.abort(); request = null; busy = false; error = null;
		if (selected) { selected = null; preview = null; }
		else { project = null; files = null; }
	}
	async function openProject(item: CurseForgeProject) {
		try {
			const url = new URL(item.links.websiteUrl ?? '');
			if (url.protocol !== 'https:' || url.hostname !== 'www.curseforge.com' || url.username || url.password || url.port) throw new Error();
			if (isTauri()) await shell.openExternalUrl(url.href);
			else window.open(url.href, '_blank', 'noopener,noreferrer');
		} catch { error = '无法打开 CurseForge 项目页面。'; }
	}
	function size(bytes: number) { return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KiB` : `${(bytes / 1024 / 1024).toFixed(1)} MiB`; }
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open) onClose(); }}>
	<Dialog.Overlay class="fixed inset-0 z-40 bg-black/15 backdrop-blur-[2px]" />
	<Dialog.Content class="fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100dvh-48px)] w-140 max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-border bg-popover text-popover-foreground shadow-[0_12px_48px_rgb(0_0_0/0.16)] outline-none">
		<header class="shrink-0 px-5 pt-4 pb-3">
			<Dialog.Title class="text-base font-semibold">添加模组</Dialog.Title>
			<Dialog.Description class="mt-1 text-[11px] text-muted-foreground">CurseForge · Minecraft {gameVersion} · Fabric</Dialog.Description>
			<Dialog.Close aria-label="关闭模组浏览" class="absolute top-4 right-4 grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></Dialog.Close>
		</header>
		<div class="min-h-0 overflow-y-auto overscroll-contain px-5 pb-4">
			{#if config?.configured}
				{#if project}
					<div class="mb-3 flex min-w-0 items-center gap-1.5"><button type="button" onclick={back} disabled={resources.starting} class={button} aria-label={selected ? '返回版本列表' : '返回搜索结果'}><ArrowLeft size={14} aria-hidden="true" /></button><span class="truncate text-[13px] font-medium">{project.name}</span><button type="button" class={button} aria-label="打开 CurseForge 项目页面" onclick={() => project && openProject(project)}><ExternalLink size={13} aria-hidden="true" /></button>{#if selected}<span class="ml-auto shrink-0 text-[11px] text-muted-foreground">安装计划</span>{/if}</div>
				{:else}
					<form class="mb-3 flex gap-2" onsubmit={(event) => { event.preventDefault(); search(); }}>
						<label class="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg bg-chrome-hover px-2.5 focus-within:ring-1 focus-within:ring-ring"><Search size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" /><input type="search" bind:value={query} maxlength={128} placeholder="搜索 CurseForge 模组" aria-label="搜索 CurseForge 模组" class="min-w-0 flex-1 bg-transparent text-xs outline-none" /></label><button type="submit" disabled={busy} class={button}>搜索</button>
					</form>
				{/if}
			{/if}
			{#if error}<div role="alert" class="mb-3 rounded-lg bg-destructive/5 px-3 py-2 text-xs text-destructive"><p class="wrap-break-word">{error}</p><button type="button" onclick={() => retry?.()} disabled={busy} class="mt-1 rounded px-1 py-1 text-xs text-primary focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">重新获取</button></div>{/if}
			{#if busy}<div role="status" class="flex min-h-36 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{selected ? '解析必需依赖…' : '读取资源目录…'}</div>
			{:else if config && !config.configured}
				<div class="flex min-h-44 flex-col items-center justify-center gap-2.5 text-center"><Package size={25} strokeWidth={1.5} class="text-muted-foreground" aria-hidden="true" /><p class="text-[13px]">CurseForge 接入准备中</p><p class="max-w-80 text-[11px] leading-5 text-muted-foreground">{config.issue} 服务配置完成后，即可浏览适用于此实例的模组。</p></div>
			{:else if preview}
				<div class="overflow-hidden rounded-lg border border-border bg-card"><ul class="divide-y divide-border px-3">{#each preview.items as item (item.project.id)}<li class="flex items-center gap-3 py-2"><div class="min-w-0 flex-1"><p class="truncate text-[13px]">{item.project.name}<span class="ml-2 text-[11px] text-muted-foreground">{item.direct ? '所选模组' : '必需依赖'} · {preview.files.find((file) => file.fileId === item.file.id)?.reuse ? '复用' : '下载'}</span></p><p class="mt-0.5 truncate text-[11px] text-muted-foreground">{item.file.displayName}</p></div><span class="shrink-0 text-[11px] text-muted-foreground">{size(item.file.fileLength)}</span><button type="button" class={button} aria-label={`打开 ${item.project.name} 项目页面`} onclick={() => openProject(item.project)}><ExternalLink size={13} aria-hidden="true" /></button></li>{/each}</ul></div>
				<p class="mt-2 text-xs text-muted-foreground">{preview.items.length} 个文件 · 预计下载 {size(preview.downloadBytes)}</p>
				{#if preview.optionalProjects.length}<p class="mt-2 text-[11px] text-muted-foreground">另有 {preview.optionalProjects.length} 个可选依赖，未自动加入。</p>{/if}
				<ul class="mt-3 space-y-1 text-[11px] leading-5 text-muted-foreground">{#each preview.warnings.filter((warning) => !warning.startsWith('这是只读')) as warning}<li>{warning}</li>{/each}</ul>
			{:else if project && !selected && files}
				<SelectableList class="max-h-72 overflow-y-auto rounded-lg border border-border bg-card" aria-label="兼容模组版本">
					{#each files.items as file (file.id)}<SelectableRow showCheck={false} onclick={() => plan(file)} class="h-10"><span class="min-w-0 truncate">{file.displayName}</span>{#snippet trailing()}<span>{file.releaseType === 1 ? '正式版' : file.releaseType === 2 ? 'Beta' : 'Alpha'} · {size(file.fileLength)}</span>{/snippet}</SelectableRow>{/each}
					{#if files.items.length === 0}<p class="px-2 py-6 text-center text-xs text-muted-foreground">此页没有兼容文件</p>{/if}
				</SelectableList>
				<div class="mt-2 flex justify-end gap-1"><button type="button" class={button} disabled={fileIndex === 0} onclick={() => { const previous = fileHistory.pop(); if (project && previous !== undefined) versions(project, previous); }} aria-label="上一页版本"><ChevronLeft size={14} /></button><button type="button" class={button} disabled={files.nextIndex === null} onclick={() => { if (project && files?.nextIndex != null) { fileHistory.push(fileIndex); versions(project, files.nextIndex); } }} aria-label="下一页版本"><ChevronRight size={14} /></button></div>
			{:else if !project && projects}
				<SelectableList class="max-h-80 overflow-y-auto rounded-lg border border-border bg-card" aria-label="CurseForge 模组项目">
					{#each projects.items as item (item.id)}<SelectableRow showCheck={false} onclick={() => versions(item)} class="h-12"><span class="min-w-0"><span class="block truncate text-[13px]">{item.name}</span><span class="mt-0.5 block truncate text-[11px] text-muted-foreground">{item.authors.map((author) => author.name).join(', ')} · {item.summary}</span></span>{#snippet trailing()}<ChevronRight size={13} aria-hidden="true" />{/snippet}</SelectableRow>{/each}
					{#if projects.items.length === 0}<p class="px-2 py-6 text-center text-xs text-muted-foreground">没有找到适用于此实例的模组</p>{/if}
				</SelectableList>
				<div class="mt-2 flex justify-end gap-1"><button type="button" class={button} disabled={projectIndex === 0} onclick={() => { const previous = projectHistory.pop(); if (previous !== undefined) search(previous, submitted); }} aria-label="上一页项目"><ChevronLeft size={14} /></button><button type="button" class={button} disabled={projects.nextIndex === null} onclick={() => { if (projects?.nextIndex != null) { projectHistory.push(projectIndex); search(projects.nextIndex, submitted); } }} aria-label="下一页项目"><ChevronRight size={14} /></button></div>
			{/if}
		</div>
		<footer class="shrink-0 border-t border-border px-5 py-3">
			{#if resources.error}<p role="alert" class="mb-2 text-[11px] text-destructive">{resources.error}</p>{/if}
			<div class="flex items-center justify-between gap-3"><p class="text-[11px] text-muted-foreground">{preview ? planExpired ? '计划已过期，请重新生成。' : '确认后下载并启用本次模组及必需依赖。' : '按实例版本与 Fabric 标签筛选。'}</p><div class="flex shrink-0 gap-1"><Dialog.Close class={button}>关闭</Dialog.Close>{#if preview}<button type="button" disabled={resources.starting || busy} onclick={() => selected && plan(selected)} class={button}>重新生成</button><button type="button" disabled={planExpired || busy || !resources.canSubmit(preview.planId)} onclick={install} class="inline-flex h-7 items-center rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground disabled:opacity-40">{resources.starting ? '提交中…' : resources.uncertain && resources.intent?.id === preview.planId ? '重试提交' : '确认安装'}</button>{/if}</div></div>
		</footer>
	</Dialog.Content>
</Dialog.Root>
