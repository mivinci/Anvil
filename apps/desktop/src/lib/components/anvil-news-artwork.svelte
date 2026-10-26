<script lang="ts">
	import type { AnvilNewsArtwork } from '../anvil-news';
	let { variant }: { variant: AnvilNewsArtwork } = $props();
</script>

<svg viewBox="0 0 560 340" fill="none" class="h-full w-full" aria-hidden="true">
	<circle cx="432" cy="82" r="48" fill="currentColor" opacity=".12" />
	<circle cx="432" cy="82" r="67" stroke="currentColor" stroke-opacity=".1" />
	<path d="M190 92H291M338 38H393M476 143H535" stroke="currentColor" stroke-opacity=".18" stroke-width="2" stroke-linecap="round" />
	{#if variant === 'blocks'}
		<path d="m250 246 142-82 143 82-143 82Z" fill="currentColor" opacity=".04" />
		{#each [{ x: 304, y: 151, size: 62 }, { x: 397, y: 98, size: 72 }, { x: 429, y: 222, size: 52 }] as block}
			<g transform={`translate(${block.x} ${block.y})`}>
				<path d={`M0 0 ${block.size} ${-block.size / 2} ${block.size * 2} 0 ${block.size} ${block.size / 2}Z`} fill="currentColor" opacity=".2" />
				<path d={`M0 0 ${block.size} ${block.size / 2} ${block.size} ${block.size * 1.5} 0 ${block.size}Z`} fill="currentColor" opacity=".09" />
				<path d={`M${block.size} ${block.size / 2} ${block.size * 2} 0 ${block.size * 2} ${block.size} ${block.size} ${block.size * 1.5}Z`} fill="currentColor" opacity=".14" />
				<path d={`M0 0 ${block.size} ${-block.size / 2} ${block.size * 2} 0V${block.size}L${block.size} ${block.size * 1.5} 0 ${block.size}ZM0 0 ${block.size} ${block.size / 2} ${block.size * 2} 0M${block.size} ${block.size / 2}V${block.size * 1.5}`} stroke="currentColor" stroke-opacity=".28" stroke-linejoin="round" />
			</g>
		{/each}
	{:else}
		<path d="M166 248 330 85 400 167 452 116 560 237V340H166Z" fill="currentColor" opacity=".07" />
		<path d="m289 181 54-41 57 64 55-30 105 94v72H205Z" fill="currentColor" opacity=".08" />
		<path d="M251 260 353 205 409 237 478 183 560 255V340H251Z" fill="currentColor" opacity=".09" />
		<path d="m330 85 70 82 52-51M353 205l56 32 69-54" stroke="currentColor" stroke-opacity=".22" stroke-linecap="round" stroke-linejoin="round" />
		{#if variant === 'journey'}
			<path d="m367 169 53-30 53 30v84l-53 30-53-30Z" fill="currentColor" opacity=".09" />
			<path d="m380 177 40-23 40 23v68l-40 23-40-23Z" stroke="currentColor" stroke-opacity=".4" stroke-width="2" stroke-linejoin="round" />
			<path d="M420 154v114m-40-91 40 23 40-23" stroke="currentColor" stroke-opacity=".24" />
			<path d="m420 268-46 26 28 16-37 22" stroke="currentColor" stroke-opacity=".2" stroke-width="10" />
		{:else}
			<path d="m345 301 45-26 41 23 46-26 42 24" stroke="currentColor" stroke-opacity=".18" stroke-width="3" stroke-linecap="round" />
			<path d="M489 142v80m-20-58 40 35m-39-10 41-36" stroke="currentColor" stroke-opacity=".3" stroke-width="2" stroke-linecap="round" />
			<path d="m478 242 12-7 12 7-12 7Z" fill="currentColor" opacity=".2" />
		{/if}
	{/if}
	<path d="m273 53 4 7 7 4-7 4-4 7-4-7-7-4 7-4Z" fill="currentColor" opacity=".25" />
</svg>
