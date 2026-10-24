<script lang="ts">
	let { id }: { id: string } = $props();
	const motif = $derived(Array.from(id).reduce((hash, character) => (hash * 31 + character.charCodeAt(0)) >>> 0, 0) % 3);
</script>
<svg viewBox="0 0 200 90" fill="none" aria-hidden="true" class="h-full w-full">
				<path d="M32 68 100 29 168 68 100 87Z" fill="currentColor" opacity=".06" />
				{#each [{ x: 52, y: 34 }, { x: 100, y: 13 + motif * 4 }, { x: 124, y: 48 }] as block}
					<g transform={`translate(${block.x} ${block.y})`}>
						<path d="M0 0 22 -12 44 0 22 12Z" fill="currentColor" opacity=".3" />
						<path d="M0 0 22 12 22 36 0 24Z" fill="currentColor" opacity=".16" />
						<path d="M22 12 44 0 44 24 22 36Z" fill="currentColor" opacity=".42" />
						<path d="M0 0 22 -12 44 0 44 24 22 36 0 24Z M0 0 22 12 44 0 M22 12V36" stroke="currentColor" stroke-opacity=".45" stroke-width=".8" stroke-linejoin="round" />
					</g>
				{/each}
			</svg>
