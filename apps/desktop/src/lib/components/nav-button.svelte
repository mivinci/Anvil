<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import { cn } from '../utils';

	let { selected = false, children, class: className, ...rest }: HTMLButtonAttributes & {
		selected?: boolean;
		children: Snippet;
	} = $props();
</script>

<button
	type="button"
	aria-current={selected ? 'page' : undefined}
	class={cn(
		'relative grid size-9 place-items-center [clip-path:url(#nav-button-shape)] transition-colors',
		selected
			? 'bg-chrome-selected text-accent-foreground'
			: 'bg-transparent text-chrome-muted/80 hover:bg-chrome-selected hover:text-chrome-muted',
		className,
	)}
	{...rest}
>
	{@render children()}
</button>
