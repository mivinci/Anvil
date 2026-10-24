<script lang="ts">
	import { resolve } from '$app/paths';
	import { Play, LoaderCircle, X } from '@lucide/svelte';
	import type { InstalledClient } from '../../runtime';
	import { getClientLaunchState, launchActive, launchLabel } from '../../client-launch-state.svelte.js';
	import { getAccountState } from '../../account-state.svelte.js';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	let { client, compact = false }: { client: InstalledClient; compact?: boolean } = $props();
	const launches = getClientLaunchState();
	const accounts = getAccountState();
	const installs = getClientInstallState();
	let hint = $state<string | null>(null);
	let checking = $state(false);
	const operation = $derived(launches.forClient(client.name));
	const active = $derived(launchActive(operation));
	const pending = $derived(checking || launches.pending.includes(client.name));
	const label = $derived(pending ? '准备中' : active && operation ? launchLabel(operation) : '启动');
	const preparing = $derived(pending || (active && operation?.phase !== 'running'));
	const buttonLabel = $derived(pending ? '准备中' : active ? operation?.phase === 'running' ? '运行中' : operation?.phase === 'java' ? operation.javaStage === 'downloading' ? '下载 Java' : '准备 Java' : '启动中' : '启动');
	const canCancel = $derived(active && operation?.phase !== 'running');
	const downloading = $derived(active && operation?.javaStage === 'downloading');
	const progress = $derived(downloading && operation?.javaTotalBytes ? Math.min(100, operation.javaDownloadedBytes / operation.javaTotalBytes * 100) : null);
	async function launch(installJava = false, offlineTest = false) {
		checking = true; hint = null;
		try {
			if (offlineTest || accounts.offlineMode) { await launches.start(client.name, null, installJava, true); return; }
			if (!accounts.loaded && !await accounts.refreshAccounts()) { hint = '无法读取账户，请稍后重试。'; return; }
			const account = accounts.currentAccount;
			if (!account) { accounts.openLoginDialog(() => { void launch(installJava, true); }); return; }
			await launches.start(client.name, account.id, installJava);
		} finally { checking = false; }
	}
</script>
<div class={compact ? 'contents' : 'relative z-20 min-w-0'}>
<div class="relative z-20 flex shrink-0 flex-wrap items-center gap-1">
				<button type="button" title={label} aria-label={`${client.name}：${label}`} disabled={active || pending || launches.uncertain || installs.active} onclick={() => void launch()} class={`inline-flex shrink-0 items-center justify-center gap-2 rounded-lg bg-primary text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50 ${compact ? 'size-7' : 'h-8 w-28 px-4 text-xs font-medium'}`}>
					{#if pending || (active && operation?.phase !== 'running')}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Play size={13} fill="currentColor" aria-hidden="true" />{/if}{#if !compact}<span class="min-w-0 truncate">{buttonLabel}</span>{/if}
				</button>
				<button type="button" aria-label="取消启动" title="取消启动" aria-hidden={!canCancel} tabindex={canCancel ? 0 : -1} disabled={!canCancel || operation?.cancelRequested} onclick={() => operation && void launches.cancel(operation)} class={`grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover disabled:opacity-50 ${canCancel ? '' : 'invisible pointer-events-none'}`}><X size={12} aria-hidden="true" /></button>
			</div>
	<div class={compact ? 'contents' : preparing || launches.uncertain || hint || operation?.error ? 'mt-2' : 'contents'}>
	{#if preparing && !compact}<p role="status" class="mb-1 min-h-4 text-[11px] text-muted-foreground">{label}</p><progress aria-label="启动准备进度" max="100" value={operation?.javaStage === 'completed' ? 100 : progress ?? undefined} class="block h-1 w-full max-w-60 accent-primary"></progress>{/if}
	{#if launches.uncertain}
		<div class="relative z-20 col-span-2 mt-2 flex items-center gap-2 px-1 text-[11px] text-muted-foreground"><span>{launches.error ?? '正在确认运行状态…'}</span><button type="button" class="text-primary" onclick={() => void launches.sync()}>刷新状态</button></div>
	{:else if !preparing && (hint || operation?.error)}
		<div class="relative z-20 col-span-2 mt-2 flex flex-wrap items-center gap-2 px-1 text-[11px] text-muted-foreground" role="status">
			<span>{hint ?? operation?.error}</span>
			{#if operation?.errorCode === 'game_exit'}<a class="text-primary" href={`${resolve('/clients/[name]/logs', { name: client.name.toLowerCase() })}?source=console`}>查看游戏日志</a>{/if}
			{#if operation?.errorCode === 'java_required'}<button type="button" disabled={pending || installs.active} class="text-primary disabled:opacity-50" onclick={() => void launch(true, operation?.offlineTest ?? false)}>安装 Java {operation.javaMajor} 并{operation.offlineTest ? '离线启动' : '启动'}</button>
			{:else if operation?.errorCode === 'account_required' || operation?.errorCode === 'authentication_failed'}<button type="button" class="text-primary" onclick={() => accounts.openLoginDialog()}>重新登录</button>
			{:else if operation?.errorCode === 'installation_conflict'}<button type="button" disabled={!installs.canStart} class="text-primary disabled:opacity-50" onclick={() => void installs.start(client.name, client.versionId, client.loader)}>修复游戏文件</button>{/if}
		</div>
	{/if}
	</div>
</div>
