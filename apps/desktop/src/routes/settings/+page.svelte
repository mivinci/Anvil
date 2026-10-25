<script lang="ts">
	import { Download, Gamepad2, Info, Palette, Search, UserRound } from '@lucide/svelte';
	import AppearanceSettings from '#lib/components/settings/appearance-settings.svelte';
	import AboutSettings from '#lib/components/settings/about-settings.svelte';
	import SelectableRow from '#lib/components/ui/selectable-row.svelte';
	const categories = [
		{ id: 'appearance', label: '外观', icon: Palette, keywords: '主题 配色 浅色 深色 系统 模式 毛玻璃', description: '调整 Anvil 的显示模式与视觉风格。' },
		{ id: 'game', label: '游戏', icon: Gamepad2, keywords: 'Java 内存 路径 目录 实例', description: '管理新建游戏实例的默认配置。' },
		{ id: 'downloads', label: '下载', icon: Download, keywords: '网络 并发 限速 缓存', description: '管理资源下载与本地缓存。' },
		{ id: 'accounts', label: '账户', icon: UserRound, keywords: '登录 Microsoft 微软 切换 移除', description: '管理这台设备上的 Minecraft 账户。' },
		{ id: 'about', label: '关于', icon: Info, keywords: 'Anvil 版本 Runtime 项目', description: '查看应用信息与项目地址。' },
	] as const;
	let selected = $state<string>('appearance');
	let query = $state('');
	const matches = $derived(categories.filter((item) => `${item.label} ${item.keywords}`.toLowerCase().includes(query.trim().toLowerCase())));
	const current = $derived(categories.find((item) => item.id === selected) ?? categories[0]);
	const pending = {
		game: '游戏目录、Java 与内存默认值将在实例配置接口接入后提供。',
		downloads: '下载并发、限速和缓存管理将在下载设置接口接入后提供。',
		accounts: '账户管理页面正在接入。目前可从窗口左下角账户菜单操作；真实登录等待 Minecraft API 应用审核。',
	};
	function search(value: string) {
		query = value;
		const first = categories.find((item) => `${item.label} ${item.keywords}`.toLowerCase().includes(value.trim().toLowerCase()));
		if (first && !matches.some((item) => item.id === selected)) selected = first.id;
	}
</script>

<svelte:head><title>设置 · Anvil</title></svelte:head>

<div class="grid min-h-0 flex-1 grid-cols-[168px_minmax(0,1fr)] overflow-hidden lg:grid-cols-[184px_minmax(0,1fr)]">
	<aside class="flex min-h-0 flex-col border-r border-border bg-secondary/30 px-2.5 py-4" aria-label="设置分类">
		<h1 class="px-2 text-[13px] font-semibold tracking-tight">设置</h1>
		<label class="mt-3 flex h-8 items-center gap-2 rounded-md bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring">
			<Search size={13} class="shrink-0" aria-hidden="true" />
			<input type="search" aria-label="搜索设置分类" placeholder="搜索设置" value={query} oninput={(event) => search(event.currentTarget.value)} class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" />
		</label>
		<nav class="mt-4 min-h-0 flex-1 space-y-0.5 overflow-y-auto overscroll-contain" aria-label="设置">
			<p class="mb-2 px-2 text-[11px] font-medium text-muted-foreground">应用设置</p>
			{#each matches as item}
				{@const Icon = item.icon}
				<SelectableRow selected={selected === item.id} density="comfortable" showCheck={false} aria-current={selected === item.id ? 'page' : undefined} onclick={() => selected = item.id} class={selected === item.id ? 'font-medium' : 'text-muted-foreground hover:text-foreground'}><Icon size={14} strokeWidth={1.7} aria-hidden="true" />{item.label}</SelectableRow>
			{:else}<p class="px-2 py-3 text-[13px] leading-5 text-muted-foreground">没有匹配的设置分类</p>{/each}
		</nav>
	</aside>
	<section class="min-h-0 min-w-0 overflow-y-auto overscroll-contain" aria-labelledby="settings-title">
		<div class="mx-auto w-full max-w-3xl px-5 py-6 lg:px-7">
			{#if selected === 'about'}<h2 id="settings-title" class="sr-only">关于</h2>
			{:else}<header class="mb-5"><h2 id="settings-title" class="text-xl font-semibold tracking-tight">{current.label}</h2></header>{/if}
			{#if selected === 'appearance'}<AppearanceSettings />
			{:else if selected === 'about'}<AboutSettings />
			{:else}
				<div class="rounded-xl border border-border bg-card p-4"><p class="text-[13px] font-medium">尚未开放</p><p class="mt-2 text-[11px] leading-5 text-muted-foreground">{pending[selected as keyof typeof pending]}</p></div>
			{/if}
		</div>
	</section>
</div>
