<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	type Props = Omit<HTMLButtonAttributes, 'class'> & {
		class?: string;
		variant?: 'default' | 'outline' | 'ghost';
		size?: 'default' | 'sm' | 'icon';
		children?: Snippet;
	};

	let {
		class: className,
		variant = 'default',
		size = 'default',
		children,
		...rest
	}: Props = $props();

	const variantClasses = {
		default: 'bg-primary text-primary-foreground shadow-sm hover:bg-primary/90',
		outline: 'border border-input bg-background hover:bg-accent hover:text-accent-foreground',
		ghost: 'hover:bg-accent hover:text-accent-foreground',
	};

	const sizeClasses = {
		default: 'h-9 px-4 py-2',
		sm: 'h-8 rounded-md px-3 text-xs',
		icon: 'size-9 p-0',
	};
</script>

<button
	class={[
		'inline-flex shrink-0 items-center justify-center gap-2 rounded-lg text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50',
		variantClasses[variant],
		sizeClasses[size],
		className,
	]}
	{...rest}
>
	{@render children?.()}
</button>
