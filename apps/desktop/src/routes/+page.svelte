<script lang="ts">
	import HomeContinue from '../lib/components/home-continue.svelte';
	import HomeInstances from '../lib/components/home-instances.svelte';
	import HomeNews from '../lib/components/home-news.svelte';
	import HomePlayerCard from '../lib/components/home-player-card.svelte';
</script>

<svelte:head><title>首页 · Anvil</title></svelte:head>

<main aria-label="首页" class="min-h-0 flex-1 overflow-y-auto p-3 lg:px-4">
	<div class="mx-auto grid w-full max-w-[1400px] grid-cols-1 gap-3 min-[900px]:grid-cols-[228px_minmax(0,1fr)_264px] min-[1200px]:grid-cols-[232px_minmax(0,1fr)_264px]">
		<HomePlayerCard />
		<div class="min-w-0"><HomeContinue /></div>
		<HomeNews maxItems={3} />
		<div class="min-w-0 min-[900px]:col-span-3"><HomeInstances /></div>
	</div>
</main>
