<script lang="ts">
	import { X, RefreshCw, Package } from '@lucide/svelte';
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';
	const resources = getResourceInstallState();
	const noun = $derived(resources.operation?.kind === 'shader' ? '光影' : '模组');
	const percent = $derived(resources.operation ? Math.min(100, resources.operation.progress.downloadedBytes / Math.max(1, resources.operation.progress.totalBytes) * 100) : 0);
	function bytes(value: number) { return `${(value / 1024 / 1024).toFixed(1)} MiB`; }
</script>
<section aria-label={`${noun}安装任务`} class="text-xs">
	<div class="flex items-center gap-2"><Package size={14} class="text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate font-medium">{resources.label}</span><button type="button" aria-label={`刷新${noun}任务`} disabled={resources.syncing || resources.starting} onclick={() => resources.sync()} class="grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover disabled:opacity-40"><RefreshCw size={12} aria-hidden="true" /></button></div>
	{#if resources.operation}<p class="mt-1 truncate text-[11px] text-muted-foreground">{resources.operation.name} · {resources.operation.progress.completedFiles}/{resources.operation.progress.totalFiles} 个文件</p><div role="progressbar" aria-label={`${noun}文件准备进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)} class="mt-2 h-1 overflow-hidden rounded-full bg-chrome-hover"><div class="h-full rounded-full bg-primary transition-[width] motion-reduce:transition-none" style:width={`${percent}%`}></div></div><p class="mt-1.5 text-[11px] text-muted-foreground">{bytes(resources.operation.progress.downloadedBytes)} / {bytes(resources.operation.progress.totalBytes)}{#if resources.bytesPerSecond !== null} · {bytes(resources.bytesPerSecond)}/s{/if}</p>{/if}
	{#if resources.error || resources.operation?.error}<p role="alert" class="mt-2 break-words text-[11px] leading-5 text-destructive">{resources.error ?? resources.operation?.error}</p>{/if}
	{#if resources.uncertain && resources.intent && !resources.active}<div class="mt-2 flex justify-end"><button type="button" disabled={resources.starting || resources.syncing} onclick={() => { const intent = resources.intent; if (intent) void resources.start(intent.name, intent.id); }} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40"><RefreshCw size={12} aria-hidden="true" />重试提交</button></div>{/if}
	{#if resources.active}<div class="mt-2 flex justify-end"><button type="button" disabled={resources.cancelling || resources.operation?.cancelRequested || resources.operation?.progress.stage === 'publishing'} onclick={() => resources.cancel()} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40"><X size={12} aria-hidden="true" />取消安装</button></div>{/if}
</section>
