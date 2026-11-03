<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { X } from '@lucide/svelte';
	import { getClientLaunchState, launchActive, launchLabel } from '../../client-launch-state.svelte.js';
	import { formatBytes as formatDownloadBytes } from '../../download-progress';
	const formatBytes = (value: number) => formatDownloadBytes(value, localeState.current);
	import type { ClientLaunch } from '../../runtime';
	let { operation }: { operation: ClientLaunch } = $props();
	const launches = getClientLaunchState();
	const active = $derived(launchActive(operation) && !['running', 'stopping'].includes(operation.phase));
	const completed = $derived(operation.javaStage === 'completed' || ['running', 'stopping', 'exited'].includes(operation.phase));
	const failed = $derived(operation.phase === 'failed' && operation.errorCode === 'java_failed');
	const downloading = $derived(active && operation.javaStage === 'downloading');
	const percent = $derived(downloading && operation.javaTotalBytes ? Math.min(100, Math.max(0, operation.javaDownloadedBytes / operation.javaTotalBytes * 100)) : null);
</script>
<div class="space-y-2.5">
	<div class="flex items-center justify-between gap-2"><div class="min-w-0"><p class="text-[13px] font-medium">Java {operation.javaMajor ?? ''}</p><p class="mt-1 truncate text-[11px] text-muted-foreground">{operation.name} · {localeState.messages.clients.launchDependency}</p></div>{#if active}<button type="button" aria-label={localeState.messages.clients.cancelJava(operation.name)} title={localeState.messages.clients.cancelJavaTitle} disabled={operation.cancelRequested} onclick={() => void launches.cancel(operation)} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><X size={13} aria-hidden="true" /></button>{/if}</div>
	<p role="status" class={`min-h-4 text-[12px] ${failed ? 'text-destructive' : 'text-muted-foreground'}`}>{failed ? localeState.messages.clients.flow.launchFailed : completed ? localeState.messages.clients.flow.javaReady : launchLabel(operation)}</p>
	{#if failed && operation.error}<details class="text-[11px] text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{operation.error}</p></details>{/if}
	<progress value={completed ? 100 : active ? percent ?? undefined : percent ?? 0} max="100" aria-label={localeState.messages.clients.flow.javaInstall} class="block h-1.5 w-full overflow-hidden rounded-full accent-primary [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary"></progress>
	<p class="min-h-4 text-[11px] tabular-nums text-muted-foreground">{#if downloading}{formatBytes(operation.javaDownloadedBytes)}{operation.javaTotalBytes !== null ? ` / ${formatBytes(operation.javaTotalBytes)}` : ''}{:else if completed}{localeState.messages.clients.javaReusable}{/if}</p>
</div>
