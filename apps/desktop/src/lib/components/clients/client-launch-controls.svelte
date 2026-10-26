<script lang="ts">
	import { resolve } from '$app/paths';
	import { Play, LoaderCircle, X } from '@lucide/svelte';
	import type { InstalledClient } from '../../runtime';
	import { getClientLaunchState, launchActive, launchLabel, launchNeedsLogin } from '../../client-launch-state.svelte.js';
	import { getAccountState } from '../../account-state.svelte.js';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	let { client, compact = false, large = false, feedback = !compact, reserveCancel = true }: {
		client: InstalledClient; compact?: boolean; large?: boolean; feedback?: boolean; reserveCancel?: boolean;
	} = $props();
	const launches = getClientLaunchState();
	const accounts = getAccountState();
	const installs = getClientInstallState();
	let hint = $state<string | null>(null);
	let checking = $state(false);
	let authenticationAttemptAfter = $state<number | null>(null);
	const operation = $derived(launches.forClient(client.name));
	const active = $derived(launchActive(operation));
	const pending = $derived(checking || launches.pending.includes(client.name));
	const idleLabel = $derived(large ? '开始游戏' : '启动');
	const label = $derived(pending ? '准备中' : active && operation ? launchLabel(operation) : idleLabel);
	const preparing = $derived(pending || (active && operation?.phase !== 'running'));
	const buttonLabel = $derived.by(() => {
		if (pending) return '准备中';
		if (!active) return idleLabel;
		if (operation?.phase === 'running') return '运行中';
		return label.replace(/^正在/, '').replace(/…$/, '').replace('Minecraft 启动信息', '启动信息').replace('Minecraft 文件', '游戏文件');
	});
	const canCancel = $derived(active && operation?.phase !== 'running');
	const downloading = $derived(active && operation?.javaStage === 'downloading');
	const progress = $derived(downloading && operation?.javaTotalBytes ? Math.min(100, operation.javaDownloadedBytes / operation.javaTotalBytes * 100) : null);
	// React only to a launch initiated here, not historical failures restored on mount.
	$effect(() => {
		if (authenticationAttemptAfter === null || !operation || operation.id <= authenticationAttemptAfter) return;
		if (launchNeedsLogin(operation)) {
			authenticationAttemptAfter = null;
			accounts.openLoginDialog(() => { void launch(false, true); });
		} else if (!active || operation.phase === 'running') authenticationAttemptAfter = null;
	});
	async function launch(installJava = false, offlineTest = false) {
		checking = true; hint = null;
		try {
			if (offlineTest || accounts.offlineMode) { await launches.start(client.name, null, installJava, true); return; }
			if (!accounts.loaded && !await accounts.refreshAccounts()) { hint = '无法读取账户，请稍后重试。'; return; }
			const account = accounts.currentAccount;
			if (!account) { accounts.openLoginDialog(() => { void launch(installJava, true); }); return; }
			authenticationAttemptAfter = operation?.id ?? 0;
			await launches.start(client.name, account.id, installJava);
		} finally { checking = false; }
	}
</script>
<div class={compact ? 'contents' : 'relative z-20 min-w-0'}>
<div class="relative z-20 flex shrink-0 flex-wrap items-center gap-1">
				<button type="button" title={label} aria-label={`${client.name}：${label}${preparing && progress !== null ? ` ${Math.floor(progress)}%` : ''}`} aria-busy={preparing} disabled={active || pending || launches.uncertain || installs.active} onclick={() => void launch()} class={`relative isolate inline-flex shrink-0 items-center justify-center gap-2 overflow-hidden rounded-lg bg-primary text-primary-foreground transition-opacity enabled:hover:opacity-90 focus-visible:outline-2 focus-visible:outline-ring ${preparing ? '' : 'disabled:opacity-50'} ${compact ? 'size-7' : large ? `h-9 max-w-full px-3.5 text-xs font-medium ${preparing ? 'w-44' : 'w-30'}` : `h-8 max-w-full px-3 text-xs font-medium ${preparing ? 'w-40' : 'w-28'}`}`}>
					{#if preparing}<span aria-hidden="true" class={`pointer-events-none absolute inset-y-0 left-0 -z-10 bg-primary-foreground/15 ${progress === null ? 'launch-progress-indeterminate w-1/3' : 'transition-[width] duration-300 motion-reduce:transition-none'}`} style:width={progress !== null ? `${progress}%` : undefined}></span>{/if}
					{#if preparing}<LoaderCircle size={large ? 14 : 13} class="shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Play size={large ? 14 : 13} fill="currentColor" class="shrink-0" aria-hidden="true" />{/if}{#if !compact}<span class="min-w-0 truncate">{buttonLabel}</span>{#if preparing && progress !== null}<span class="shrink-0 text-[10px] tabular-nums">{Math.floor(progress)}%</span>{/if}{/if}
				</button>
				{#if canCancel || reserveCancel}<button type="button" aria-label="取消启动" title="取消启动" aria-hidden={!canCancel} tabindex={canCancel ? 0 : -1} disabled={!canCancel || operation?.cancelRequested} onclick={() => operation && void launches.cancel(operation)} class={`grid shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover disabled:opacity-50 ${large ? 'size-7' : 'size-6'} ${canCancel ? '' : 'invisible pointer-events-none'}`}><X size={large ? 13 : 12} aria-hidden="true" /></button>{/if}
			</div>
	{#if feedback}
	<div class={!compact && (launches.uncertain || hint || operation?.error) ? 'mt-2' : 'contents'}>
	{#if preparing && !compact}<span role="status" class="sr-only">{label}</span><progress aria-label="启动准备进度" max="100" value={progress ?? undefined} class="sr-only"></progress>{/if}
	{#if launches.uncertain}
		<div class="relative z-20 col-span-2 mt-2 flex items-center gap-2 px-1 text-[11px] text-muted-foreground"><span>{launches.error ?? '正在确认运行状态…'}</span><button type="button" class="text-primary" onclick={() => void launches.sync()}>刷新状态</button></div>
	{:else if !preparing && (hint || operation?.error)}
		<div class="relative z-20 col-span-2 mt-2 flex flex-wrap items-center gap-2 px-1 text-[11px] text-muted-foreground" role="status">
			<span>{hint ?? operation?.error}</span>
			{#if operation?.errorCode === 'game_exit'}<a class="text-primary" href={`${resolve('/clients/[name]/logs', { name: client.name.toLowerCase() })}?source=console`}>查看游戏日志</a>{/if}
			{#if operation?.errorCode === 'java_required'}<button type="button" disabled={pending || installs.active} class="text-primary disabled:opacity-50" onclick={() => void launch(true, operation?.offlineTest ?? false)}>安装 Java {operation.javaMajor} 并{operation.offlineTest ? '离线启动' : '启动'}</button>
			{:else if launchNeedsLogin(operation)}<button type="button" class="text-primary" onclick={() => accounts.openLoginDialog()}>重新登录</button>
			{:else if operation?.errorCode === 'installation_conflict'}<button type="button" disabled={!installs.canStart} class="text-primary disabled:opacity-50" onclick={() => void installs.start(client.name, client.versionId, client.loader)}>修复游戏文件</button>{/if}
		</div>
	{/if}
	</div>
	{/if}
</div>

<style>
	.launch-progress-indeterminate {
		animation: launch-progress 1.6s ease-in-out infinite;
	}
	@keyframes launch-progress {
		from { transform: translateX(-100%); }
		to { transform: translateX(300%); }
	}
	@media (prefers-reduced-motion: reduce) {
		.launch-progress-indeterminate { animation: none; transform: translateX(100%); }
	}
</style>
