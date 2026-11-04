<script lang="ts">
	import { runtimeErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.js';
	import { X, RefreshCw, Package, Pause, Play } from '@lucide/svelte';
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';
	import type { ResourceInstallOperation } from '../../runtime';
	let { operation = null }: { operation?: ResourceInstallOperation | null } = $props();
	const resources = getResourceInstallState();
	const transferCopy = $derived(localeState.messages.clients.flow);
	const noun = $derived(operation ? resources.nounFor(operation) : localeState.messages.mods.discovery.resourceMod);
	const percent = $derived(operation ? Math.min(100, operation.progress.downloadedBytes / Math.max(1, operation.progress.totalBytes) * 100) : 0);
	const issue = $derived(operation ? resources.taskIssues[operation.id] : undefined);
	const speed = $derived(operation ? resources.speeds[operation.id] : undefined);
	const error = $derived(operation
		? issue ? localeState.messages.mods.discovery[issue.key]
			: operation.error ? runtimeErrorMessage(operation.errorCode, localeState.current, localeState.messages.mods.discovery.resourceFailed) : null
		: resources.error);
	const detail = $derived(operation ? issue?.detail ?? operation.error : resources.errorDetail);
	function bytes(value: number) { return `${new Intl.NumberFormat(localeState.current, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value / 1024 / 1024)} MiB`; }
</script>

<section aria-label={operation ? `${localeState.messages.mods.taskLabel(noun)} · ${operation.name}` : localeState.messages.mods.discovery.resourceTask} class="text-xs">
	<div class="flex items-center gap-2">
		<Package size={14} class="text-muted-foreground" aria-hidden="true" />
		<span class="min-w-0 flex-1 truncate font-medium">{operation ? resources.labelFor(operation) : resources.label}</span>
		<button type="button" aria-label={localeState.messages.mods.refreshTask(noun)} disabled={resources.syncing || resources.starting} onclick={() => resources.sync()} class="grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover disabled:opacity-40"><RefreshCw size={12} aria-hidden="true" /></button>
	</div>
	{#if operation}
		<p class="mt-1 truncate text-xs text-muted-foreground" title={operation.name}>{operation.name} · {localeState.messages.mods.fileProgress(operation.progress.completedFiles, operation.progress.totalFiles)}</p>
		<div role="progressbar" aria-label={localeState.messages.mods.fileProgressLabel(noun)} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)} class="mt-2 h-1 overflow-hidden rounded-full bg-chrome-hover"><div class="h-full rounded-full bg-primary transition-[width] motion-reduce:transition-none" style:width={`${percent}%`}></div></div>
		<p class="mt-1.5 text-xs text-muted-foreground">{bytes(operation.progress.downloadedBytes)} / {bytes(operation.progress.totalBytes)}{#if speed !== undefined} · {bytes(speed)}/s{/if}</p>
	{/if}
	{#if error}<p role="alert" class="mt-2 break-words text-xs leading-5 text-destructive">{error}</p>{/if}
	{#if detail}<details class="mt-2 text-xs text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{detail}</p></details>{/if}
	{#if !operation && resources.uncertain && resources.intent}
		<div class="mt-2 flex justify-end"><button type="button" disabled={resources.starting || resources.syncing} onclick={() => { const intent = resources.intent; if (intent) void resources.start(intent.name, intent.id); }} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40"><RefreshCw size={12} aria-hidden="true" />{localeState.messages.mods.discovery.retrySubmit}</button></div>
	{/if}
	{#if operation?.state === 'running'}
		{@const task = operation}
		<div class="mt-2 flex flex-wrap justify-end gap-1">
			{#if task.paused}
				<button type="button" disabled={!resources.canResume(task)} onclick={() => void resources.setPaused(task.id, false)} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Play size={12} aria-hidden="true" />{transferCopy.resumeDownload}</button>
			{:else if resources.supportsPause(task) && (task.progress.stage === 'downloading' || task.pauseRequested)}
				<button type="button" disabled={!resources.canPause(task)} onclick={() => void resources.setPaused(task.id, true)} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Pause size={12} aria-hidden="true" />{task.pauseRequested ? transferCopy.pausing : transferCopy.pauseDownload}</button>
			{/if}
			<button type="button" disabled={!resources.canCancel(task)} onclick={() => void resources.cancel(task.id)} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40"><X size={12} aria-hidden="true" />{localeState.messages.mods.discovery.cancelInstall}</button>
		</div>
	{/if}
</section>
