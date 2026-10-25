<script lang="ts">
	import HomeNews from '../lib/components/home-news.svelte';
	import HomePlay from '../lib/components/home-play.svelte';
	import HomePlayerCard from '../lib/components/home-player-card.svelte';
</script>

<svelte:head><title>首页 · Anvil</title></svelte:head>

<main aria-label="首页" class="min-h-0 flex-1 overflow-y-auto p-3 lg:px-4">
	<div class="mx-auto grid w-full max-w-[1400px] grid-cols-1 items-start gap-3 min-[900px]:grid-cols-[228px_minmax(0,1fr)] min-[1200px]:grid-cols-[232px_minmax(0,1fr)_264px]">
		<HomePlayerCard />
		<HomePlay />
		<HomeNews class="min-[900px]:col-span-2 min-[1200px]:col-span-1 min-[1200px]:col-start-3 min-[1200px]:row-start-1" />
	</div>
</main>
