<script lang="ts">
	import { onMount } from 'svelte';
	import { Dialog } from 'bits-ui';
	import { LoaderCircle, LogIn, RefreshCw, Trash2, UserRound } from '@lucide/svelte';
	import AccountAvatar from '../account-avatar.svelte';
	import { getAccountState } from '../../account-state.svelte.js';
	import { getClientLaunchState, launchActive } from '../../client-launch-state.svelte.js';
	import { PlayStatsState } from '../../play-stats-state.svelte.js';
	import { formatPlayCount, formatPlayDuration } from '../../play-stats';
	import type { PlayStatsClearTarget } from '../../runtime';

	const accounts = getAccountState();
	const launches = getClientLaunchState();
	const stats = new PlayStatsState();
	const current = $derived(accounts.currentAccount);
	const history = $derived(current ? stats.snapshot?.accounts[current.id] : null);
	const playing = $derived(launches.pending.length > 0 || launches.operations.some(launchActive));
	const blocked = $derived(playing || launches.uncertain || stats.loading || stats.clearing || !stats.snapshot || stats.available === false);
	let confirmation = $state<{ target: PlayStatsClearTarget; username: string | null } | null>(null);
	let message = $state<string | null>(null);
	const rowButton = 'inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	const secondary = 'h-8 rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	onMount(() => { void stats.refresh(); return () => stats.dispose(); });

	function openConfirmation(target: PlayStatsClearTarget) {
		if (blocked || stats.clearUncertain) return;
		message = null;
		stats.clearError = null;
		confirmation = { target, username: target.kind === 'account' ? current?.username ?? null : null };
	}

	function finish(target: PlayStatsClearTarget, reconciled = false) {
		const clearedCache = target.kind !== 'all' || launches.clearRecentHistory();
		message = !clearedCache ? '游玩记录已清除，但最近实例缓存未能清除。'
			: reconciled ? '已确认所选范围的游玩记录为空。'
			: target.kind === 'all' ? '已清除本机全部游玩记录。' : `已清除 ${confirmation?.username ?? '该账户'} 的游玩记录。`;
		confirmation = null;
	}

	async function confirm() {
		if (!confirmation || blocked || stats.clearUncertain) return;
		const { target } = confirmation;
		if (await stats.clear(target)) finish(target);
	}

	async function reconcile() {
		const target = confirmation?.target;
		if (!target || !await stats.refresh()) return;
		const empty = target.kind === 'all' ? Object.keys(stats.snapshot?.accounts ?? {}).length === 0 : !stats.snapshot?.accounts[target.accountId];
		if (empty) finish(target, true);
		else { confirmation = null; message = '已重新读取记录，请核对后再发起清除。'; }
	}
</script>

<section aria-label="当前账户" class="flex items-center justify-between gap-3 rounded-lg bg-card p-3.5">
	<div class="flex min-w-0 items-center gap-3">
		{#if current}<AccountAvatar account={current} size={36} />{:else}<span class="grid size-9 shrink-0 place-items-center rounded-md bg-chrome-hover text-muted-foreground"><UserRound size={19} aria-hidden="true" /></span>{/if}
		<div class="min-w-0"><p class="truncate text-[13px] font-medium">{current?.username ?? '尚未登录账户'}</p><p class="mt-0.5 text-[11px] text-muted-foreground">{current ? 'Microsoft 账户' : '登录后可单独管理账户的游玩记录'}</p></div>
	</div>
	<button type="button" class={`${rowButton} text-muted-foreground hover:bg-chrome-hover hover:text-foreground`} disabled={accounts.busy} onclick={() => accounts.openLoginDialog()}><LogIn size={13} aria-hidden="true" />{current ? '添加账户' : '登录'}</button>
</section>

<section aria-labelledby="play-history-title" class="mt-6">
	<div class="mb-2 flex items-center justify-between gap-2"><h3 id="play-history-title" class="text-[13px] font-medium">游玩记录</h3><button type="button" class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={stats.loading || stats.clearing} onclick={() => void stats.refresh()} aria-label="刷新游玩记录"><RefreshCw size={13} class={stats.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button></div>
	<p class="mb-3 text-[11px] leading-5 text-muted-foreground">记录保存在这台设备上，删除游戏实例后仍会保留。</p>
	<div class="divide-y divide-border/60 rounded-lg bg-card px-3.5">
		<div class="flex items-center justify-between gap-4 py-3">
			<div class="min-w-0"><p class="text-[13px] font-medium">当前账户的游玩记录</p><p class="mt-1 text-[11px] leading-5 text-muted-foreground">{!current ? '请先登录或选择账户。' : stats.loading ? '正在读取记录…' : stats.error || stats.available === false ? '游玩记录暂不可用。' : history ? `${current.username} · ${formatPlayCount(history.launches)}启动 · ${formatPlayDuration(history.totalMs)}` : `${current.username} · 暂无游玩记录`}</p></div>
			<button type="button" class={`${rowButton} text-destructive enabled:hover:bg-destructive/10`} disabled={blocked || !current || !history || stats.clearUncertain} onclick={() => current && openConfirmation({ kind: 'account', accountId: current.id })}><Trash2 size={13} aria-hidden="true" />清除记录</button>
		</div>
		<div class="flex items-center justify-between gap-4 py-3">
			<div class="min-w-0"><p class="text-[13px] font-medium">本机全部游玩记录</p><p class="mt-1 text-[11px] leading-5 text-muted-foreground">包括其他账户、离线记录和最近游玩列表。</p></div>
			<button type="button" class={`${rowButton} text-muted-foreground enabled:hover:bg-destructive/10 enabled:hover:text-destructive`} disabled={blocked || stats.clearUncertain || (!Object.keys(stats.snapshot?.accounts ?? {}).length && !launches.recentPlays.length)} onclick={() => openConfirmation({ kind: 'all' })}>全部清除</button>
		</div>
	</div>
	{#if playing}<p class="mt-2 text-[11px] leading-5 text-muted-foreground">请先结束正在启动或运行的游戏，再清除记录。</p>
	{:else if launches.uncertain}<p class="mt-2 text-[11px] leading-5 text-muted-foreground">运行状态尚未确认。<button type="button" class="text-primary hover:underline" onclick={() => void launches.sync()}>刷新状态</button></p>{/if}
	{#if stats.available === false}<p class="mt-2 text-[11px] text-muted-foreground">当前版本暂不支持游玩记录管理。</p>{/if}
	{#if stats.error}<p role="alert" class="mt-2 text-[11px] leading-5 text-destructive">{stats.error}</p>{/if}
	{#if stats.clearUncertain && !confirmation}<p role="alert" class="mt-2 text-[11px] leading-5 text-muted-foreground">{stats.clearError} <button type="button" class="text-primary hover:underline" disabled={stats.loading} onclick={() => void stats.refresh()}>重新读取记录</button></p>{/if}
	{#if message}<p role="status" class="mt-2 text-[11px] leading-5 text-muted-foreground">{message}</p>{/if}
</section>

<Dialog.Root open={confirmation !== null} onOpenChange={open => { if (!open && !stats.clearing) confirmation = null; }}>
	<Dialog.Portal to=".app-window">
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
		<Dialog.Content class="fixed top-1/2 left-1/2 z-50 w-[min(360px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-popover p-4 text-popover-foreground shadow-lg outline-none" onEscapeKeydown={event => { if (stats.clearing) event.preventDefault(); }} onInteractOutside={event => { if (stats.clearing) event.preventDefault(); }}>
			<Dialog.Title class="text-sm font-semibold">{confirmation?.target.kind === 'all' ? '清除本机全部游玩记录？' : `清除 ${confirmation?.username ?? '此账户'} 的游玩记录？`}</Dialog.Title>
			<Dialog.Description class="mt-2 text-xs leading-6 text-muted-foreground">{confirmation?.target.kind === 'all' ? '这台设备上所有账户和离线游玩的累计时长、启动次数、历史会话与最近游玩列表将被清除。' : '仅清除此账户在这台设备上的累计时长、启动次数和历史会话。其他账户与离线记录会保留。'} 此操作无法撤销，不会删除游戏实例、存档或登录账户。</Dialog.Description>
			{#if stats.clearError}<p role="alert" class="mt-3 text-xs leading-5 text-destructive">{stats.clearError}</p>{/if}
			<div class="mt-4 flex justify-end gap-1.5">
				<button type="button" class={secondary} disabled={stats.clearing} onclick={() => confirmation = null}>取消</button>
				{#if stats.clearUncertain}<button type="button" class={secondary} disabled={stats.loading || stats.clearing} onclick={() => void reconcile()}>{stats.loading ? '正在读取…' : '重新读取记录'}</button>
				{:else}<button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-lg bg-destructive px-3 text-xs font-medium text-white focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={blocked} onclick={() => void confirm()}>{#if stats.clearing}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />清除中…{:else}确认清除{/if}</button>{/if}
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
