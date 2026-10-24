<script lang="ts">
	import { X } from '@lucide/svelte';
	import { getClientLaunchState, launchActive, launchLabel } from '../../client-launch-state.svelte.js';
	import { formatBytes } from '../../download-progress';
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
	<div class="flex items-center justify-between gap-2"><div class="min-w-0"><p class="text-[13px] font-medium">Java {operation.javaMajor ?? ''}</p><p class="mt-1 truncate text-[11px] text-muted-foreground">{operation.name} · 启动依赖</p></div>{#if active}<button type="button" aria-label={`取消 ${operation.name} 的 Java 安装并取消启动`} title="取消安装并取消启动" disabled={operation.cancelRequested} onclick={() => void launches.cancel(operation)} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><X size={13} aria-hidden="true" /></button>{/if}</div>
	<p role="status" class={`min-h-4 text-[12px] ${failed ? 'text-destructive' : 'text-muted-foreground'}`}>{failed ? operation.error : completed ? 'Java 已准备完成' : launchLabel(operation)}</p>
	<progress value={completed ? 100 : active ? percent ?? undefined : percent ?? 0} max="100" aria-label="Java 安装进度" class="block h-1.5 w-full overflow-hidden rounded-full accent-primary [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary"></progress>
	<p class="min-h-4 text-[11px] tabular-nums text-muted-foreground">{#if downloading}{formatBytes(operation.javaDownloadedBytes)}{operation.javaTotalBytes !== null ? ` / ${formatBytes(operation.javaTotalBytes)}` : ''}{:else if completed}托管 Java 可供其他实例复用{/if}</p>
</div>
