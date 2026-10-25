<script lang="ts">
	import { onMount } from 'svelte';
	import { Check, ChevronLeft, Download, LoaderCircle, RefreshCw, Search } from '@lucide/svelte';
	import ModProjectIcon from '../mods/mod-project-icon.svelte';
	import { formatBytes } from '../../download-progress';
	import { formatModDownloads, formatModUpdated } from '../../mod-project-stats';
	import { createShaderInstallPlan, getShaderProject, listClientShaderPacks, listShaderVersions, searchShaderProjects, type ModProjectSummary, type ModrinthFile, type ShaderInstallPlan } from '../../runtime';
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';

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
	let error = $state<string | null>(null);
	let message = $state<string | null>(null);
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
			if (request === controller && !controller.signal.aborted) error = reason instanceof Error ? reason.message : '无法读取实例光影清单。';
		} finally {
			if (request === controller) { request = null; loadingRevision = false; }
		}
	}
	async function search(index: number) {
		const controller = begin();
		searching = true;
		pending = null;
		plan = null;
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
		pending = null;
		plan = null;
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
		pending = null;
		plan = null;
		error = null;
		message = null;
	}
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
		error = null;
		message = null;
		try {
			const value = await createShaderInstallPlan(name, String(selected.id), file.id, revision, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (request !== controller || controller.signal.aborted) return;
			plan = value;
		} catch (reason) {
			if (request === controller && !controller.signal.aborted) error = reason instanceof Error ? reason.message : '无法生成下载计划。';
		} finally {
			if (request === controller) { request = null; planning = false; }
		}
	}
	async function install() {
		if (!plan || !resources.canSubmit(plan.planId)) return;
		error = null;
		message = null;
		if (!(await resources.start(name, plan.planId))) error = resources.error;
	}
	async function cancelOperation() {
		await resources.cancel();
		if (resources.error) error = resources.error;
	}
	function resetPending() {
		abort();
		pending = null;
		plan = null;
		error = null;
		message = null;
	}
	function downloads() {
		document.querySelector<HTMLButtonElement>('[data-download-tasks-trigger]')?.click();
	}
	$effect(() => {
		const value = operation;
		if (!value || !plan || handled === value.id) return;
		if (value.state === 'completed') {
			handled = value.id;
			message = '光影已添加到此实例，可在游戏内选择。';
			pending = null;
			plan = null;
			void loadRevision();
			onImported?.();
		} else if (value.state === 'failed' || value.state === 'cancelled') {
			handled = value.id;
			error = value.error ?? (value.state === 'cancelled' ? '下载已取消。' : '下载失败，请重新生成计划。');
			plan = null;
		}
	});
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
				<div class="mt-3 space-y-0.5" aria-label="选择兼容版本">
					{#each versions as file (file.id)}
						<button type="button" aria-pressed={pending?.id === file.id} disabled={planning || resources.starting || resources.active} onclick={() => void choose(file)} class={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring disabled:opacity-40 ${pending?.id === file.id ? 'bg-chrome-selected' : 'hover:bg-chrome-hover'}`}>
							<span class="min-w-0 flex-1"><span class="block truncate text-xs font-medium" title={file.displayName}>{file.displayName}</span><span class="mt-1 block truncate text-[11px] text-muted-foreground" title={file.fileName}>{file.fileDate.slice(0, 10)} · {releaseLabel(file.releaseType)} · {formatBytes(file.fileLength)}</span></span>
							{#if pending?.id === file.id}<Check size={14} class="shrink-0 text-primary" aria-hidden="true" />{/if}
						</button>
					{/each}
				</div>
				<button type="button" class="mt-2 flex w-full items-center justify-center gap-1 rounded-lg py-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40" disabled={loadingVersions} onclick={() => { if (selected) void select(selected); }}><RefreshCw size={12} aria-hidden="true" />刷新版本</button>
			{/if}
		{/if}
	</div>
	{#if pending}
		<section aria-label="确认光影下载" class="mt-3 shrink-0 rounded-lg bg-card px-3 py-2.5">
			<div class="flex flex-wrap items-center gap-x-3 gap-y-2">
				<div class="min-w-0 flex-1 text-xs">
					{#if planning}
						<p role="status" class="flex items-center gap-2 text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在生成下载计划…</p>
					{:else if operation?.state === 'running'}
						<p role="status" class="flex items-center gap-2 text-muted-foreground">{#if operation.cancelRequested}正在取消…{:else if operation.progress.stage === 'publishing'}正在应用光影…{:else if operation.progress.stage === 'downloading'}正在下载并校验光影{percent !== null ? ` ${percent}%` : ''}…{:else}正在准备光影文件…{/if}</p>
						<div class="mt-1.5 h-1 overflow-hidden rounded-full bg-chrome-hover"><div class="h-full rounded-full bg-primary transition-[width] duration-300" style:width={`${percent ?? 0}%`}></div></div>
					{:else if error}
						<p role="alert" class="break-words text-destructive">{error}</p>
					{:else if plan}
						<p class="truncate" title={`${plan.projectName} · ${plan.versionName} · ${formatBytes(plan.fileSize)}`}><span class="text-muted-foreground">安装到</span> {name} <span class="text-muted-foreground">· {plan.projectName} · {plan.versionName} · {formatBytes(plan.fileSize)}</span></p>
						{#if resources.active || resources.starting}<p class="mt-0.5 text-[10px] text-muted-foreground">等待其他资源任务完成后再提交。</p>{/if}
					{:else}
						<p class="truncate text-muted-foreground">已选择 {pending.displayName}，正在准备确认信息…</p>
					{/if}
				</div>
				{#if planning}
					<span class="text-[10px] text-muted-foreground">请稍候</span>
				{:else if operation?.state === 'running'}
					<button type="button" class={button} onclick={downloads}>查看任务</button>
					<button type="button" class={button} disabled={resources.cancelling || operation.cancelRequested} onclick={() => void cancelOperation()}>{resources.cancelling || operation.cancelRequested ? '取消中…' : '取消'}</button>
				{:else if error}
					<button type="button" class={button} onclick={resetPending}>取消</button>
					<button type="button" class={button} disabled={!pending || revision === null} onclick={() => pending && void choose(pending)}><RefreshCw size={13} aria-hidden="true" />重新生成计划</button>
				{:else if plan}
					<button type="button" class={button} onclick={resetPending}>取消</button>
					<button type="button" disabled={!resources.canSubmit(plan.planId)} onclick={() => void install()} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if resources.starting}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />提交中…{:else}<Download size={13} aria-hidden="true" />确认下载并导入{/if}</button>
				{/if}
			</div>
		</section>
	{/if}

</div>
