<script lang="ts">
	import { untrack } from 'svelte';
	import { LoaderCircle, Check, Download, ChevronUp, RefreshCw } from '@lucide/svelte';
	import { Portal } from 'bits-ui';
	import { modPlanWarnings } from '../../mod-plan-warnings';
	import { ModCatalogState } from '../../mod-catalog-state.svelte';
	import { ModInstallPlanState, type ModPlanSnapshot, type ModInstallFile } from '../../mod-install-plan';
	import { getResourceInstallState } from '../../resource-install-state.svelte';
	import { formatBytes } from '../../download-progress';
	import { listModrinthFiles, listCurseForgeFiles, createModrinthInstallPlan, createCurseForgeInstallPlan,
		type ModSource, type CurseForgePage } from '../../runtime';
	let { source, name, gameVersion, project, footerTarget, blocked = false }: { source: ModSource; name: string; gameVersion: string; project: string; footerTarget: HTMLElement | undefined; blocked?: boolean } = $props();
	const resources = getResourceInstallState();
	const files = new ModCatalogState<CurseForgePage<ModInstallFile>>();
	let index = $state(0), history = $state<number[]>([]);
	let review = $state<ModPlanSnapshot>({ selected: null, plan: null, busy: false, error: null, expired: false, accepted: false });
	const plans = new ModInstallPlanState({
		read: async (context, file, signal) => {
			if (context.source === 'modrinth' && typeof file.id === 'string') return createModrinthInstallPlan(context.name, context.project, file.id, signal);
			if (context.source === 'curseforge' && typeof file.id === 'number') return createCurseForgeInstallPlan(context.name, Number(context.project), file.id, signal);
			throw new Error('模组来源与版本标识不匹配。');
		},
		start: (target, id) => resources.start(target, id), canSubmit: id => !blocked && resources.canSubmit(id),
	}, value => { review = value; });
	const button = 'inline-flex items-center justify-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	const reused = $derived(review.plan?.files.filter(file => file.reuse).length ?? 0);
	const operation = $derived(resources.operation?.id === review.plan?.planId ? resources.operation : null);
	const accepted = $derived(review.accepted || operation !== null);
	const planLocked = $derived(resources.starting || (resources.uncertain && resources.intent !== null));
	const dependencies = $derived(review.plan?.items.filter(item => !item.direct).length ?? 0);
	const warnings = $derived(modPlanWarnings(review.plan?.warnings ?? []));
	let detailsOpen = $state(false);
	const detailsId = $props.id();
	$effect(() => { review.selected?.id; review.plan?.planId; detailsOpen = false; });
	function load(force = false) {
		const current = source, target = name, id = project, offset = index;
		return files.load((signal, options) => current === 'modrinth' ? listModrinthFiles(target, id, offset, signal, options) : listCurseForgeFiles(target, Number(id), offset, signal, options), force);
	}
	$effect(() => {
		const context = { source, name, gameVersion, project };
		untrack(() => { index = 0; history = []; plans.bind(context); });
		return () => plans.dispose();
	});
	$effect(() => {
		source; name; gameVersion; project; index;
		untrack(() => { plans.clear(); files.clear(); void load(); });
		return () => files.cancel();
	});
	function downloads() { document.querySelector<HTMLButtonElement>('[data-download-tasks-trigger]')?.click(); }
</script>

<div class="flex min-h-0 flex-1 flex-col">
	{#if files.busy}<p role="status" class="mt-3 flex items-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取兼容版本…</p>{/if}
	{#if files.error || files.value?.refreshError}<p role="alert" class="mt-3 text-xs text-destructive">{files.error ?? files.value?.refreshError}{files.value ? '（保留上次版本）' : ''}<button type="button" class={button} disabled={files.busy || planLocked} onclick={() => { plans.clear(); void load(true); }}>重试</button></p>{/if}
	<div class="mt-3 min-h-0 flex-1 space-y-0.5 scrollbar-subtle overflow-y-auto overscroll-contain" aria-label="选择兼容版本">
		{#each files.value?.items ?? [] as file (file.id)}
			<button type="button" aria-pressed={review.selected?.id === file.id} disabled={blocked || planLocked} onclick={() => void plans.select(file)} class={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring disabled:opacity-40 ${review.selected?.id === file.id ? 'bg-chrome-selected' : 'hover:bg-chrome-hover'}`}>
				<span class="min-w-0 flex-1"><span class="block truncate text-xs font-medium" title={file.displayName}>{file.displayName}</span><span class="mt-1 block truncate text-[11px] text-muted-foreground" title={file.fileName}>{file.fileDate.slice(0, 10)} · {file.releaseType === 1 ? 'Release' : file.releaseType === 2 ? 'Beta' : 'Alpha'} · {formatBytes(file.fileLength)}</span></span>
				{#if review.selected?.id === file.id}<Check size={14} class="shrink-0 text-primary" aria-hidden="true" />{/if}
			</button>
		{:else}{#if !files.busy && !files.error}<p class="px-2 py-5 text-xs text-muted-foreground">暂时没有适用于此实例的版本。</p>{/if}{/each}
	</div>
	{#if history.length || files.value?.nextIndex != null}<div class="mt-2 flex justify-end gap-1"><button type="button" class={button} disabled={files.busy || planLocked || !history.length} onclick={() => { index = history.at(-1)!; history = history.slice(0, -1); }}>上一页</button><button type="button" class={button} disabled={files.busy || planLocked || files.value?.nextIndex == null} onclick={() => { history = [...history, index]; index = files.value!.nextIndex!; }}>下一页</button></div>{/if}
</div>

{#if footerTarget && (review.selected || resources.active || resources.starting || resources.uncertain || resources.error)}
<Portal to={footerTarget}>
	<section aria-label="确认模组安装计划" class="bg-card">
		{#if review.plan && detailsOpen}
			<div id={detailsId} class="max-h-[min(35dvh,16rem)] scrollbar-subtle overflow-y-auto overscroll-contain border-b border-border px-4 py-2.5">
				<ul class="divide-y divide-border">{#each review.plan.items as item (item.project.id)}
					<li class="flex items-center gap-2 py-2"><div class="min-w-0 flex-1"><p class="truncate text-xs" title={item.project.name}>{item.project.name}<span class="ml-2 text-[10px] text-muted-foreground">{item.direct ? '所选模组' : '必需依赖'}</span></p><p class="mt-0.5 truncate text-[11px] text-muted-foreground" title={item.file.fileName}>{item.file.displayName}</p></div><span class="shrink-0 text-right text-[10px] leading-4 text-muted-foreground">{review.plan.files.find(file => file.projectId === item.project.id && file.fileId === item.file.id)?.reuse ? '复用缓存' : '下载'}<br />{formatBytes(item.file.fileLength)}</span></li>
				{/each}</ul>
				<p class="mt-2 text-[11px] text-muted-foreground">{review.plan.files.length} 个文件 · 复用 {reused} 个</p>
				{#if review.plan.optionalProjects.length}<p class="mt-2 text-[11px] leading-5 text-muted-foreground">{review.plan.optionalProjects.length} 个可选依赖未加入安装。</p>{/if}
				{#each warnings.details as warning}<p class="mt-2 text-[11px] leading-5 text-muted-foreground">{warning}</p>{/each}
			</div>
		{/if}
		<div class="flex flex-wrap items-center gap-x-2 gap-y-2 px-3 py-2.5">
			<div class="min-w-0 flex-1 text-xs">
				{#if review.busy}<p role="status" class="flex items-center gap-2 text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在解析安装计划…</p>
				{:else if review.error}<p role="alert" class="break-words text-destructive">{review.error}</p>
				{:else if accepted}<p role="status" class={operation?.state === 'failed' ? 'text-destructive' : 'text-muted-foreground'}>{operation?.state === 'completed' ? '安装已完成' : operation?.state === 'cancelled' ? '安装已取消' : operation?.state === 'failed' ? '安装失败' : '已提交，可在下载任务查看进度'}</p>
				{:else if review.expired}<p role="status" class="text-destructive">计划已过期</p>
				{:else if review.plan}<p class="truncate" title={`安装到 ${name} · 下载 ${formatBytes(review.plan.downloadBytes)}`}><span class="text-muted-foreground">安装到</span> {name} <span class="text-muted-foreground">· 下载 {formatBytes(review.plan.downloadBytes)}</span></p>{#if dependencies}<p class="mt-0.5 text-[10px] text-muted-foreground">含 {dependencies} 个必需依赖</p>{/if}
				{:else if resources.active}<p class="truncate text-muted-foreground">正在为 {resources.operation?.name} 安装模组</p>{/if}
			</div>
			{#if review.plan}<button type="button" class={`${button} size-8 shrink-0 px-0`} aria-label={detailsOpen ? '收起安装详情' : '展开安装详情'} title={detailsOpen ? '收起安装详情' : '依赖与文件详情'} aria-expanded={detailsOpen} aria-controls={detailsId} onclick={() => detailsOpen = !detailsOpen}><ChevronUp size={15} class={detailsOpen ? 'rotate-180' : ''} aria-hidden="true" /></button>{/if}
			{#if accepted}
				{#if operation?.state === 'failed' || operation?.state === 'cancelled'}<button type="button" class={button} disabled={blocked || planLocked} onclick={() => review.selected && void plans.select(review.selected)}>重新生成</button>{/if}<button type="button" class={button} onclick={downloads}>查看任务</button>
			{:else if review.error || review.expired}<button type="button" class={button} disabled={blocked || review.busy || planLocked || (resources.uncertain && resources.intent !== null)} onclick={() => review.selected && void plans.select(review.selected)}><RefreshCw size={13} aria-hidden="true" />重新生成</button>
			{:else if review.plan}<button type="button" disabled={blocked || review.busy || !resources.canSubmit(review.plan.planId)} onclick={() => void plans.confirm()} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Download size={13} aria-hidden="true" />{resources.starting ? '提交中…' : resources.uncertain && resources.intent?.id === review.plan.planId ? '重试提交' : '确认安装'}</button>{/if}
		</div>
		{#if warnings.important.length}<div role="alert" class="px-3 pb-2.5 text-[11px] leading-5 text-destructive">{#each warnings.important as warning}<p>{warning}</p>{/each}</div>{/if}
		{#if resources.error || (resources.active && !accepted) || resources.uncertain}
			<div class="flex flex-wrap items-center gap-x-2 gap-y-1 border-t border-border px-3 py-2 text-[11px] text-muted-foreground" aria-label="窗口安装任务状态">
				<p class="min-w-0 flex-1 break-words">{resources.error ?? (resources.active ? `正在为 ${resources.operation?.name} 安装，请等待当前任务完成。` : '正在确认任务状态…')}</p>
				{#if resources.error && review.plan && !accepted && !resources.uncertain && !resources.active && !review.expired}<button type="button" class={button} disabled={blocked || review.busy || planLocked} onclick={() => review.selected && void plans.select(review.selected)}>重新生成</button>{/if}
				<button type="button" class={button} onclick={downloads}>查看任务</button>{#if resources.uncertain}<button type="button" class={button} disabled={resources.syncing || resources.starting} onclick={() => void resources.sync()}>刷新状态</button>{/if}
			</div>
		{/if}
	</section>
</Portal>
{/if}
