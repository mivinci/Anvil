<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import { Check } from '@lucide/svelte';
	import { cn } from '../../utils';

	let { selected = false, density = 'compact', showCheck = true, children, trailing,
		class: className, ...rest }: HTMLButtonAttributes & {
		selected?: boolean;
		density?: 'compact' | 'comfortable';
		showCheck?: boolean;
		children: Snippet;
		trailing?: Snippet;
	} = $props();
</script>

<button
	type="button"
	class={cn(
		'flex w-full min-w-0 items-center gap-2 rounded-md px-2.5 text-left text-[13px] leading-4 transition-colors focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-40',
		density === 'compact' ? 'h-7' : 'h-8',
		selected ? 'bg-chrome-selected text-accent-foreground' : 'text-foreground hover:bg-chrome-hover',
		className,
	)}
	{...rest}
>
	<span class="flex min-w-0 flex-1 items-center gap-2">{@render children()}</span>
	{#if trailing}<span class="shrink-0 text-[11px] text-muted-foreground">{@render trailing()}</span>{/if}
	{#if showCheck}<span class="grid size-[13px] shrink-0 place-items-center" aria-hidden="true">{#if selected}<Check size={13} class="text-primary" />{/if}</span>{/if}
</button>
