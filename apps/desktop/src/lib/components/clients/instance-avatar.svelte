<script lang="ts">
	import Avatar from '../ui/avatar.svelte';
	import { instanceAvatarPattern } from '../../instance-avatar';
	let { id, name, src, class: className = 'size-12' }: {
		id: string; name: string; src?: string | null; class?: string;
	} = $props();
	const pattern = $derived(instanceAvatarPattern(id));
</script>

<Avatar src={src ?? null} alt={name} class={className}>
	{#snippet fallback()}
		<svg viewBox="0 0 7 7" class="size-full" aria-hidden="true" style:color={`hsl(${pattern.hue} 42% 51%)`}>
			<rect width="7" height="7" fill="currentColor" opacity="0.14" />
			{#each pattern.cells as filled, index}
				{#if filled}<rect x={1 + index % 5} y={1 + Math.floor(index / 5)} width="0.85" height="0.85" rx="0.12" fill="currentColor" />{/if}
			{/each}
		</svg>
	{/snippet}
</Avatar>
