<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { Download, Gamepad2, Info, Palette, Search, Settings2, UserRound } from '@lucide/svelte';
	import { localeState } from '#lib/locale.svelte.ts';
	import SelectableRow from '#lib/components/ui/selectable-row.svelte';
	import type { Snippet } from 'svelte';

	let { children }: { children: Snippet } = $props();
	const text = $derived(localeState.messages.settings);
	const categories = $derived([
		{ id: 'general', label: text.general, icon: Settings2, keywords: text.generalKeywords },
		{ id: 'appearance', label: text.appearance, icon: Palette, keywords: text.appearanceKeywords },
		{ id: 'game', label: text.game, icon: Gamepad2, keywords: text.gameKeywords },
		{ id: 'downloads', label: text.downloads, icon: Download, keywords: text.downloadsKeywords },
		{ id: 'accounts', label: text.accounts, icon: UserRound, keywords: text.accountsKeywords },
		{ id: 'about', label: text.about, icon: Info, keywords: text.aboutKeywords },
	] as const);
	const section = $derived(page.url.pathname.split('/')[2] ?? 'general');
	const current = $derived(categories.find((item) => item.id === section) ?? categories[0]);
	let query = $state('');
	const matches = $derived(categories.filter((item) => `${item.label} ${item.keywords}`.toLowerCase().includes(query.trim().toLowerCase())));
	function path(id: string) {
		return `/settings/${id}`;
	}
	function search(value: string) {
		query = value;
		const next = categories.filter((item) => `${item.label} ${item.keywords}`.toLowerCase().includes(value.trim().toLowerCase()));
		const first = next[0];
		if (first && !next.some((item) => item.id === current.id)) void goto(path(first.id));
	}
</script>

<svelte:head><title>{text.title} · Anvil</title></svelte:head>

<div class="grid min-h-0 flex-1 grid-cols-[168px_minmax(0,1fr)] overflow-hidden lg:grid-cols-[184px_minmax(0,1fr)]">
	<aside class="flex min-h-0 flex-col bg-secondary/30 px-2.5 py-4" aria-label={text.categoriesLabel}>
		<h1 class="px-2 text-body font-semibold tracking-tight">{text.title}</h1>
		<label class="mt-3 flex h-8 items-center gap-2 rounded-md bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring">
			<Search size={13} class="shrink-0" aria-hidden="true" />
			<input type="search" aria-label={text.searchLabel} placeholder={text.searchPlaceholder} value={query} oninput={(event) => search(event.currentTarget.value)} class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" />
		</label>
		<nav class="mt-4 min-h-0 flex-1 space-y-0.5 overflow-y-auto overscroll-contain" aria-label={text.title}>
			<p class="mb-2 px-2 text-caption font-medium text-muted-foreground">{text.applicationSettings}</p>
			{#each matches as item}
				{@const Icon = item.icon}
				<SelectableRow selected={current.id === item.id} density="comfortable" showCheck={false} aria-current={current.id === item.id ? 'page' : undefined} onclick={() => void goto(path(item.id))} class={current.id === item.id ? 'font-medium' : 'text-muted-foreground hover:text-foreground'}><Icon size={14} strokeWidth={1.7} aria-hidden="true" />{item.label}</SelectableRow>
			{:else}<p class="px-2 py-3 text-body leading-5 text-muted-foreground">{text.noMatches}</p>{/each}
		</nav>
	</aside>
	<section class="min-h-0 min-w-0 overflow-y-auto overscroll-contain" aria-labelledby="settings-title">
		<div class="mx-auto w-full max-w-3xl px-5 py-6 lg:px-7">
			{#if current.id === 'about'}<h2 id="settings-title" class="sr-only">{current.label}</h2>
			{:else}<header class="mb-5"><h2 id="settings-title" class="text-xl font-semibold tracking-tight">{current.label}</h2></header>{/if}
			{@render children()}
		</div>
	</section>
</div>
