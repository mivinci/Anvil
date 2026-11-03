<script lang="ts">
	import { runtimeErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.ts';
	import { Check, Download, LoaderCircle, RotateCcw, TriangleAlert, X } from '@lucide/svelte';
	import { formatBytes as formatDownloadBytes } from '../../download-progress';
	const formatBytes = (value: number) => formatDownloadBytes(value, localeState.current);
	import { getModpackExportState } from '../../modpack-export-state.svelte.js';
	const exports = getModpackExportState();
	const operation = $derived(exports.operation);
	const percent = $derived(operation?.progress.totalBytes && operation.progress.totalBytes > 0
		? Math.min(100, operation.progress.processedBytes / operation.progress.totalBytes * 100)
		: operation?.progress.totalFiles && operation.progress.totalFiles > 0
			? Math.min(100, operation.progress.completedFiles / operation.progress.totalFiles * 100)
			: null);
	const button = 'inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring';
</script>

<section aria-label={localeState.messages.instance.detail.exportTaskName} class="text-xs">
	<div class="flex items-center gap-2">
		<span class={`grid size-5 shrink-0 place-items-center rounded-full ${operation?.state === 'completed' ? 'bg-primary/10 text-primary' : operation?.state === 'failed' ? 'bg-destructive/10 text-destructive' : 'bg-chrome-hover text-muted-foreground'}`}>
			{#if operation?.state === 'completed'}<Check size={12} strokeWidth={2.5} aria-hidden="true" />
			{:else if operation?.state === 'failed'}<TriangleAlert size={12} aria-hidden="true" />
			{:else if operation?.state === 'cancelled'}<X size={12} aria-hidden="true" />
			{:else}<LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{/if}
		</span>
		<p role="status" class={`min-w-0 flex-1 truncate ${operation?.state === 'failed' ? 'text-destructive' : 'text-muted-foreground'}`}>{exports.label}</p>
		{#if percent !== null && exports.active}<span class="shrink-0 text-[12px] tabular-nums text-muted-foreground">{Math.round(percent)}%</span>{/if}
	</div>
	{#if operation}
		<p class="mt-1 truncate text-[12px] text-muted-foreground" title={operation.name}>{operation.name}</p>
		{#if exports.active}
			<div class="mt-2 h-1 overflow-hidden rounded-full bg-chrome-hover"><div class="h-full rounded-full bg-primary transition-[width] duration-300 motion-reduce:transition-none" style:width={`${percent ?? 0}%`}></div></div>
			<p class="mt-1.5 text-[12px] tabular-nums text-muted-foreground">{operation.progress.totalFiles === null ? localeState.messages.instance.detail.collectingFiles : localeState.messages.instance.fileProgress(operation.progress.completedFiles, operation.progress.totalFiles)}{#if operation.progress.totalBytes !== null} · {formatBytes(operation.progress.processedBytes)} / {formatBytes(operation.progress.totalBytes)}{/if}</p>
		{/if}
		{#if operation.state === 'completed' && operation.result}
			<p class="mt-1.5 break-all text-[12px] leading-4 text-muted-foreground">{localeState.messages.instance.detail.savedTo} {operation.result.path}</p>
			<p class="mt-1 text-[12px] text-muted-foreground">{formatBytes(operation.result.size)} · {localeState.messages.instance.count(operation.result.files, 'files')}</p>
		{/if}
		{#if operation.error || exports.errorDetail}<details class="mt-2 text-[12px] text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{exports.errorDetail ?? operation.error}</p></details>{/if}
		{#if operation.error || exports.error}<p role="alert" class="mt-2 break-words text-[12px] leading-5 text-destructive">{exports.error ?? runtimeErrorMessage(operation.errorCode, localeState.current, localeState.messages.instance.detail.exportFailed)}</p>{/if}
	{/if}
	<div class="mt-2 flex flex-wrap items-center justify-end gap-1.5">
		{#if exports.active}
			<button type="button" class={button} disabled={exports.cancelling || operation?.cancelRequested} onclick={() => void exports.cancel()}>{exports.cancelling || operation?.cancelRequested ? localeState.messages.instance.detail.cancelling : localeState.messages.instance.detail.cancelExport}</button>
		{:else if operation?.state === 'failed' || operation?.state === 'cancelled' || exports.uncertain}
			<button type="button" class={button} disabled={exports.starting || exports.syncing} onclick={() => void exports.retry()}><RotateCcw size={12} aria-hidden="true" />{exports.uncertain ? localeState.messages.instance.detail.retrySubmit : localeState.messages.instance.detail.exportAgain}</button>
		{/if}
		{#if exports.error || exports.uncertain}<button type="button" class={button} disabled={exports.syncing || exports.starting} onclick={() => void exports.sync()}>{exports.syncing ? localeState.messages.instance.detail.querying : localeState.messages.instance.detail.refreshStatus}</button>{/if}
	</div>
	{#if operation?.state === 'completed'}<p class="mt-1 flex items-center gap-1 text-[12px] text-muted-foreground"><Download size={11} aria-hidden="true" />{localeState.messages.instance.detail.sharePack}</p>{/if}
</section>
