<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { ArrowUpRight, ChevronLeft, ChevronRight, Pause, Play } from '@lucide/svelte';
	import { anvilNews } from '../anvil-news';
	import AnvilNewsArtwork from './anvil-news-artwork.svelte';

	let index = $state(0);
	let paused = $state(false);
	let hovered = $state(false);
	let focused = $state(false);
	let visible = $state(true);
	let reducedMotion = $state(false);
	const autoPlaying = $derived(anvilNews.length > 1 && !paused && !hovered && !focused && visible && !reducedMotion);
	const controlClass = 'grid size-6 place-items-center rounded-md text-muted-foreground transition-colors enabled:hover:bg-primary/10 enabled:hover:text-primary focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	onMount(() => {
		const media = window.matchMedia('(prefers-reduced-motion: reduce)');
		const motionChanged = () => { reducedMotion = media.matches; };
		const visibilityChanged = () => { visible = document.visibilityState === 'visible'; };
		motionChanged(); visibilityChanged();
		media.addEventListener('change', motionChanged);
		document.addEventListener('visibilitychange', visibilityChanged);
		return () => {
			media.removeEventListener('change', motionChanged);
			document.removeEventListener('visibilitychange', visibilityChanged);
		};
	});

	function select(next: number) { index = (next + anvilNews.length) % anvilNews.length; }
	$effect(() => {
		if (!autoPlaying) return;
		const next = index + 1;
		const timer = setTimeout(() => select(next), 8_000);
		return () => clearTimeout(timer);
	});
</script>

<section class="group relative isolate h-full min-h-66 min-w-0 overflow-hidden rounded-lg bg-card" aria-labelledby="anvil-news-title" aria-roledescription="轮播"
	onmouseenter={() => hovered = true} onmouseleave={() => hovered = false}
	onfocusin={() => focused = true} onfocusout={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) focused = false; }}>
	<div class="pointer-events-none absolute inset-0 bg-primary/5" aria-hidden="true"></div>
	<div class="relative z-10 flex items-center justify-between px-4 pt-4">
		<h2 id="anvil-news-title" class="text-[11px] font-medium tracking-wide text-muted-foreground">Anvil 动态</h2>
		<span class="text-[10px] tabular-nums text-muted-foreground/70" aria-hidden="true">{String(index + 1).padStart(2, '0')} / {String(anvilNews.length).padStart(2, '0')}</span>
	</div>
	{#each anvilNews as item, position (item.id)}
		<div class={`absolute inset-0 transition-opacity duration-500 motion-reduce:transition-none ${position === index ? 'opacity-100' : 'pointer-events-none opacity-0'}`} inert={position !== index} aria-hidden={position !== index}>
			<div class="pointer-events-none absolute -top-3 -right-9 h-full w-[125%] text-primary min-[1200px]:-right-3 min-[1200px]:w-full"><AnvilNewsArtwork variant={item.artwork} /></div>
			<div class="relative flex h-full flex-col items-start justify-end px-4 pt-12 pb-13">
				<span class="mb-2 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">{item.category}</span>
				<h3 class="text-xl leading-[1.45] font-semibold tracking-tight whitespace-pre-line">{item.title}</h3>
				<p class="mt-2 max-w-66 text-[11px] leading-5 text-muted-foreground">{item.description}</p>
				<a href={resolve(item.href)} class="mt-3 inline-flex items-center gap-1 rounded-md text-[11px] font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-ring">{item.action}<ArrowUpRight size={13} aria-hidden="true" /></a>
			</div>
		</div>
	{/each}
	<div class="absolute inset-x-4 bottom-3 z-10 flex items-center justify-between gap-3">
		<div class="flex items-center gap-0.5" aria-label="选择 Anvil 动态">
			{#each anvilNews as item, position (item.id)}<button type="button" onclick={() => select(position)} aria-label={`第 ${position + 1} 条：${item.title.replace('\n', '')}`} aria-pressed={position === index} class="grid h-6 min-w-5 place-items-center rounded focus-visible:outline-2 focus-visible:outline-ring"><span class={`block h-1 rounded-full transition-[width,background-color] duration-300 motion-reduce:transition-none ${position === index ? 'w-5 bg-primary' : 'w-1 bg-primary/25'}`}></span></button>{/each}
		</div>
		<div class="flex items-center gap-0.5">
			<button type="button" class={controlClass} onclick={() => paused = !paused} disabled={reducedMotion} aria-label={reducedMotion ? '已根据系统设置停用自动轮播' : paused ? '开启自动轮播' : '暂停自动轮播'} title={reducedMotion ? '已根据系统设置停用自动轮播' : paused ? '开启自动轮播' : '暂停自动轮播'} aria-pressed={paused || reducedMotion}>{#if paused || reducedMotion}<Play size={11} aria-hidden="true" />{:else}<Pause size={11} aria-hidden="true" />{/if}</button>
			<button type="button" class={controlClass} onclick={() => select(index - 1)} aria-label="上一条动态"><ChevronLeft size={14} aria-hidden="true" /></button>
			<button type="button" class={controlClass} onclick={() => select(index + 1)} aria-label="下一条动态"><ChevronRight size={14} aria-hidden="true" /></button>
		</div>
	</div>
</section>
