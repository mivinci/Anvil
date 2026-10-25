<script lang="ts">
	import HomeHero from '../lib/components/home-hero.svelte';
	import HomeInstances from '../lib/components/home-instances.svelte';
	import HomeNews from '../lib/components/home-news.svelte';
</script>

<svelte:head><title>首页 · Anvil</title></svelte:head>

<main aria-label="首页" class="min-h-0 flex-1 overflow-y-auto p-3 lg:px-4">
	<div class="mx-auto grid w-full max-w-[1400px] grid-cols-1 gap-3 min-[900px]:grid-cols-[minmax(0,1fr)_264px]">
		<div class="min-w-0 min-[900px]:col-span-2"><HomeHero /></div>
		<HomeInstances />
		<HomeNews />
	</div>
</main>
