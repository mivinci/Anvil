<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { DropdownMenu } from 'bits-ui';
	import { Check, ChevronDown, Gamepad2, LogIn, LogOut, RefreshCw, UserRound } from '@lucide/svelte';
	import AccountAvatar from './account-avatar.svelte';
	import { getAccountState } from '../account-state.svelte.js';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState, launchActive } from '../client-launch-state.svelte.js';
	import { PlayStatsState } from '../play-stats-state.svelte.js';
	import { formatPlayCount, formatPlayDuration, mostPlayedInstance, playStatsAccount } from '../play-stats.js';

	const accounts = getAccountState();
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
	const stats = new PlayStatsState();
	const current = $derived(accounts.currentAccount);
	const accountStats = $derived(playStatsAccount(stats.snapshot, current?.id));
	const running = $derived(launches.operations.find((value) => launchActive(value)) ?? null);
	let now = $state(Date.now());
	let hadActive = false;

	const itemClass = 'flex min-h-8 cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-[12px] outline-none data-[highlighted]:bg-chrome-hover data-[disabled]:opacity-40';
	const actionClass = 'inline-flex h-7 items-center gap-1.5 rounded-lg bg-chrome-hover px-2 text-[11px] text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	function clientFor(id: string | null | undefined) {
		if (!id) return null;
		return library.clients.find((value) => value.id === id || value.name.toLowerCase() === id.toLowerCase()) ?? null;
	}
	function instanceName(id: string | null | undefined) {
		if (!id) return null;
		const client = clientFor(id);
		if (client) return client.name;
		return library.loaded ? '已删除实例' : id;
	}
	const mostPlayedLabel = $derived(instanceName(mostPlayedInstance(accountStats)) ?? '—');
	const totalLabel = $derived(accountStats && accountStats.totalMs > 0
		? formatPlayDuration(accountStats.totalMs)
		: accountStats && accountStats.launches > 0 ? '不到 1 分钟' : '—');
	const weekLabel = $derived(accountStats && accountStats.last7dMs > 0 ? formatPlayDuration(accountStats.last7dMs) : '—');
	const launchesLabel = $derived(formatPlayCount(accountStats?.launches ?? 0));
	const runningElapsed = $derived(running?.startedAt ? formatPlayDuration(now - running.startedAt) : null);
	const loadingAccount = $derived(accounts.loading && !accounts.loaded);

	onMount(() => {
		void stats.refresh();
		void library.refresh();
		return () => stats.dispose();
	});

	$effect(() => {
		if (!running) return;
		now = Date.now();
		const timer = setInterval(() => { now = Date.now(); }, 30_000);
		return () => clearInterval(timer);
	});

	$effect(() => {
		const active = launches.operations.some(launchActive);
		if (hadActive && !active) void stats.refresh();
		hadActive = active;
	});

	async function switchAccount(id: string) {
		await accounts.switchAccount(id);
	}
	async function signOut() {
		if (current) await accounts.logout(current.id);
	}
	function login() {
		accounts.openLoginDialog();
	}
</script>

<section class="flex h-full min-w-0 flex-col rounded-lg border border-border bg-card p-3" aria-labelledby="home-player-title">
	<h2 id="home-player-title" class="sr-only">玩家</h2>

	{#if current}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="group flex w-full min-w-0 items-center gap-3 rounded-lg p-1 text-left transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">
				<AccountAvatar account={current} size={56} />
				<span class="min-w-0 flex-1">
					<span class="block truncate text-[15px] font-semibold leading-5" title={current.username}>{current.username}</span>
					<span class="mt-1 inline-flex max-w-full items-center rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">Microsoft 账户</span>
				</span>
				<ChevronDown size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
			</DropdownMenu.Trigger>
			<DropdownMenu.Content side="bottom" align="start" sideOffset={6} collisionPadding={8} class="z-30 w-56 rounded-xl border border-chrome-control-border bg-popover/95 p-1 text-popover-foreground shadow-[0_4px_16px_rgb(0_0_0/0.08)] backdrop-blur-xl outline-none" aria-label="账户">
				<p class="px-2 py-1.5 text-[10px] font-medium text-chrome-muted">账户</p>
				{#each accounts.accounts as account (account.id)}
					<DropdownMenu.Item class={itemClass} disabled={accounts.busy} onSelect={() => void switchAccount(account.id)}>
						<AccountAvatar {account} size={22} />
						<span class="min-w-0 flex-1 truncate">{account.username}</span>
						{#if account.selected}<Check size={14} aria-label="当前账户" />{/if}
					</DropdownMenu.Item>
				{/each}
				<DropdownMenu.Separator class="mx-1.5 my-1 h-px bg-border/60" />
				<DropdownMenu.Item class={itemClass} disabled={accounts.busy} onSelect={login}>
					<LogIn size={14} strokeWidth={1.75} aria-hidden="true" /><span class="flex-1">添加账户</span>
				</DropdownMenu.Item>
				<DropdownMenu.Item class={itemClass} disabled={accounts.busy} onSelect={() => void signOut()}>
					<LogOut size={14} strokeWidth={1.75} aria-hidden="true" /><span class="flex-1">退出登录</span>
				</DropdownMenu.Item>
				{#if accounts.error}
					<p class="px-2 py-1 text-[10px] leading-4 text-destructive" role="alert">{accounts.error.message}</p>
				{/if}
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	{:else if loadingAccount}
		<div class="flex items-center gap-3 p-1" role="status">
			<span class="grid size-14 shrink-0 place-items-center rounded-xl bg-chrome-hover text-muted-foreground"><UserRound size={22} aria-hidden="true" /></span>
			<span class="text-[11px] text-muted-foreground">正在读取账户…</span>
		</div>
	{:else}
		<button type="button" onclick={login} class="flex w-full items-center gap-3 rounded-lg p-1 text-left transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">
			<span class="grid size-14 shrink-0 place-items-center rounded-xl bg-chrome-hover text-muted-foreground"><LogIn size={22} aria-hidden="true" /></span>
			<span class="min-w-0">
				<span class="block text-[13px] font-medium">登录 Microsoft 账户</span>
				<span class="mt-1 block text-[10px] text-muted-foreground">登录后记录游玩统计。</span>
			</span>
		</button>
	{/if}

	{#if current}
		<div class="mt-3 grid grid-cols-2 gap-2 min-[600px]:grid-cols-4 min-[900px]:grid-cols-2" aria-busy={stats.loading}>
			<div class="rounded-lg bg-chrome-hover px-2 py-1.5">
				<p class="text-[10px] text-muted-foreground">总游玩</p>
				<p class="mt-1 truncate text-[14px] font-semibold tabular-nums" title={totalLabel}>{totalLabel}</p>
			</div>
			<div class="rounded-lg bg-chrome-hover px-2 py-1.5">
				<p class="text-[10px] text-muted-foreground">近 7 天</p>
				<p class="mt-1 truncate text-[14px] font-semibold tabular-nums" title={weekLabel}>{weekLabel}</p>
			</div>
			<div class="rounded-lg bg-chrome-hover px-2 py-1.5">
				<p class="text-[10px] text-muted-foreground">启动次数</p>
				<p class="mt-1 truncate text-[14px] font-semibold tabular-nums" title={launchesLabel}>{launchesLabel}</p>
			</div>
			<div class="rounded-lg bg-chrome-hover px-2 py-1.5">
				<p class="text-[10px] text-muted-foreground">最常玩</p>
				<p class="mt-1 truncate text-[14px] font-semibold" title={mostPlayedLabel}>{mostPlayedLabel}</p>
			</div>
		</div>
	{/if}

	{#if running}
		<div class="mt-3 flex items-center gap-2 rounded-lg bg-primary/8 px-2.5 py-2 text-[11px]" role="status" aria-live="polite">
			<span class="size-1.5 shrink-0 animate-pulse rounded-full bg-emerald-500 motion-reduce:animate-none" aria-hidden="true"></span>
			<span class="min-w-0 flex-1 truncate">{running.name} · {running.offlineTest ? '离线测试' : '运行中'}{#if runningElapsed} · {runningElapsed}{/if}</span>
			<a href={resolve('/clients/[name]/logs', { name: running.name.toLowerCase() })} class="shrink-0 text-primary hover:underline">日志</a>
		</div>
	{/if}

	<div class="mt-auto flex flex-wrap items-center gap-1.5 pt-3">
		<a href={resolve('/clients')} class={actionClass}><Gamepad2 size={13} aria-hidden="true" />实例库</a>
		{#if current}
			<button type="button" class={`${actionClass} w-7 justify-center px-0`} disabled={stats.loading} onclick={() => void stats.refresh()} aria-label="刷新游玩统计" title="刷新游玩统计">
				<RefreshCw size={13} class={stats.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" />
			</button>
		{/if}
	</div>

	{#if current && stats.error}
		<p role="status" class="mt-2 text-[10px] leading-4 text-muted-foreground">统计暂不可用 <button type="button" class="text-primary hover:underline" onclick={() => void stats.refresh()}>重试</button></p>
	{/if}
</section>
