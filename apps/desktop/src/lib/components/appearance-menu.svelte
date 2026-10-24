<script module lang="ts">
	export type AppearanceMode = 'system' | 'light' | 'dark';
</script>

<script lang="ts">
	import { Monitor, Moon, Settings2, Sun } from '@lucide/svelte';
	import NavButton from './nav-button.svelte';

	let { appearance, onChange }: {
		appearance: AppearanceMode;
		onChange: (mode: AppearanceMode) => void;
	} = $props();

	const appearanceOptions: {
		value: AppearanceMode;
		label: string;
		icon: typeof Monitor;
	}[] = [
		{ value: 'system', label: '跟随系统', icon: Monitor },
		{ value: 'light', label: '浅色', icon: Sun },
		{ value: 'dark', label: '深色', icon: Moon },
	];

	let appearanceMenuOpen = $state(false);

	function selectAppearance(mode: AppearanceMode) {
		onChange(mode);
		appearanceMenuOpen = false;
	}
</script>

<div class="relative z-20">
	<NavButton
		aria-label="外观模式"
		aria-haspopup="menu"
		aria-expanded={appearanceMenuOpen}
		onclick={() => (appearanceMenuOpen = !appearanceMenuOpen)}
	><Settings2 size={19} /></NavButton>
	{#if appearanceMenuOpen}
		<div class="absolute bottom-0 left-[calc(100%+8px)] z-20 w-44 rounded-md-xl border border-chrome-control-border bg-popover p-1.5 text-popover-foreground shadow-xl" role="menu" aria-label="外观模式">
			<div class="px-2.25 pt-1.75 pb-1.25 text-[11px] font-semibold text-chrome-subtle">外观</div>
			{#each appearanceOptions as option (option.value)}
				{@const Icon = option.icon}
				<button
					class="flex h-8.5 w-full items-center gap-2.25 rounded-md-lg px-2.25 text-left text-xs text-popover-foreground transition-colors hover:bg-chrome-hover"
					class:bg-chrome-hover={appearance === option.value}
					role="menuitemradio"
					aria-checked={appearance === option.value}
					onclick={() => selectAppearance(option.value)}
				>
					<Icon size={16} />
					<span class="flex-1">{option.label}</span>
					{#if appearance === option.value}<span class="text-primary">✓</span>{/if}
				</button>
			{/each}
		</div>
	{/if}
</div>
