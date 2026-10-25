<script lang="ts">
	import { onMount } from 'svelte';
	import { ArrowUpRight, LoaderCircle, Newspaper, RefreshCw } from '@lucide/svelte';
	import { getMinecraftNews, type MinecraftNewsItem } from '../runtime';
	import { shell } from '../shell';

	let { class: className = '', maxItems = 8 }: { class?: string; maxItems?: number } = $props();
	let news = $state<MinecraftNewsItem[]>([]);
	let loading = $state(true);
	let refreshing = $state(false);
	let stale = $state(false);
	let error = $state<string | null>(null);
	const javaNews = $derived(news.filter((item) => item.categories.some((category) => /java/i.test(category))));
	const visibleNews = $derived((javaNews.length ? javaNews : news).slice(0, Math.max(1, Math.min(8, Math.trunc(maxItems) || 8))));
	const usingGeneralNews = $derived(javaNews.length === 0 && news.length > 0);

	onMount(() => { void loadNews(); });

	async function loadNews(refresh = false) {
		if (refresh) refreshing = true;
		else loading = true;
		error = null;
		try {
			const result = await getMinecraftNews({ refresh, signal: AbortSignal.timeout(15_000) });
			news = result.items;
			stale = result.stale;
			error = result.refreshError;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : '暂时无法读取 Minecraft 官方新闻。';
		} finally {
			loading = false;
			refreshing = false;
		}
	}

	function dateLabel(value: string | null) {
		if (!value) return '日期未提供';
		const date = new Date(value);
		return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat('zh-CN', { year: 'numeric', month: 'short', day: 'numeric' }).format(date);
	}

	async function openArticle(item: MinecraftNewsItem) {
		try { await shell.openExternalUrl(item.url); }
		catch { error = '无法打开文章链接，请稍后重试。'; }
	}
</script>

<aside class={`min-w-0 ${className}`} aria-labelledby="news-title">
	<section class="rounded-lg border border-border bg-card p-3">
		<div class="mb-1 flex items-center justify-between gap-2">
			<div class="flex min-w-0 items-center gap-2">
				<Newspaper size={15} class="shrink-0 text-primary" aria-hidden="true" />
				<h2 id="news-title" class="truncate text-[13px] font-semibold">{usingGeneralNews ? 'Minecraft 动态' : 'Java 版动态'}</h2>
			</div>
			<div class="flex shrink-0 items-center gap-1">
				{#if stale}<span class="px-1 text-[10px] text-muted-foreground">缓存</span>{/if}
				<button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground disabled:opacity-50" aria-label="刷新新闻" title="刷新新闻" disabled={loading || refreshing} onclick={() => void loadNews(true)}>
					{#if refreshing}<LoaderCircle size={14} class="animate-spin" aria-hidden="true" />{:else}<RefreshCw size={14} aria-hidden="true" />{/if}
				</button>
			</div>
		</div>

		{#if loading && news.length === 0}
			<div class="flex min-h-24 items-center justify-center text-muted-foreground" role="status"><LoaderCircle size={16} class="animate-spin" aria-hidden="true" /></div>
		{:else if visibleNews.length > 0}
			<ul class="divide-y divide-border/70">
				{#each visibleNews as item (item.url)}
					<li>
						<button type="button" class="group block w-full py-2.5 text-left" onclick={() => void openArticle(item)}>
							<span class="line-clamp-2 text-xs font-medium leading-5 text-foreground transition-colors group-hover:text-primary">{item.title}</span>
							<span class="mt-1 flex items-center justify-between gap-2 text-[10px] text-muted-foreground"><span>{dateLabel(item.publishedAt)}</span><ArrowUpRight size={12} class="shrink-0 transition-colors group-hover:text-primary" aria-hidden="true" /></span>
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<div class="flex min-h-24 items-center justify-center text-xs text-muted-foreground" role="status">
				{error ? '动态暂不可用' : '暂无动态'}
			</div>
		{/if}

		{#if error && visibleNews.length > 0}<p class="mt-2 truncate text-[10px] text-muted-foreground" role="status" title={error}>刷新失败</p>{/if}
	</section>
</aside>
