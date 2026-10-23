<script lang="ts">
	import { Blocks, Gamepad2, House, Server } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import type { AppearanceMode } from './appearance-menu.svelte';
	import AccountMenu from './account-menu.svelte';
	import GamepadFilled from './icons/gamepad-filled.svelte';
	import NavButton from './nav-button.svelte';

	let { appearance, onAppearanceChange }: {
		appearance: AppearanceMode;
		onAppearanceChange: (mode: AppearanceMode) => void;
	} = $props();
</script>

<svg class="pointer-events-none absolute size-0" aria-hidden="true" focusable="false">
	<defs>
		<clipPath id="nav-button-shape" clipPathUnits="objectBoundingBox">
			<!-- Symmetric Bézier segments keep tangent direction and curvature continuous at each join. -->
			<path d="M .5 0 C .93 0 1 .07 1 .5 C 1 .93 .93 1 .5 1 C .07 1 0 .93 0 .5 C 0 .07 .07 0 .5 0 Z" />
		</clipPath>
	</defs>
</svg>

<nav class="flex min-h-0 flex-col items-center bg-transparent py-2" aria-label="主导航">
	<div class="flex flex-col items-center gap-2">
		<NavButton aria-label="首页" selected={page.url.pathname === resolve('/')} onclick={() => goto(resolve('/'))}><House size={19} /></NavButton>
		<NavButton aria-label="客户端" selected={page.url.pathname === resolve('/clients') || page.url.pathname.startsWith(`${resolve('/clients')}/`)} onclick={() => goto(resolve('/clients'))}>
			{#if page.url.pathname === resolve('/clients') || page.url.pathname.startsWith(`${resolve('/clients')}/`)}
				<GamepadFilled />
			{:else}
				<Gamepad2 size={19} />
			{/if}
		</NavButton>
		<NavButton aria-label="服务端" selected={page.url.pathname === resolve('/servers') || page.url.pathname.startsWith(`${resolve('/servers')}/`)} onclick={() => goto(resolve('/servers'))}><Server size={19} /></NavButton>
		<NavButton aria-label="模组与整合包" selected={page.url.pathname === resolve('/mods') || page.url.pathname.startsWith(`${resolve('/mods')}/`)} onclick={() => goto(resolve('/mods'))}><Blocks size={19} /></NavButton>
	</div>
	<div class="mt-auto flex flex-col items-center gap-2.5">
		<AccountMenu {appearance} {onAppearanceChange} />
	</div>
</nav>
