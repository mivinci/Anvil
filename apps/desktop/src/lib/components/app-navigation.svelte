<script lang="ts">
	import { Blocks, Gamepad2, House, Server, Sparkles } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import type { AppearanceMode } from './appearance-menu.svelte';
	import AccountMenu from './account-menu.svelte';
	import UpdateNavButton from './update-nav-button.svelte';
	import BlocksFilled from './icons/blocks-filled.svelte';
	import GamepadFilled from './icons/gamepad-filled.svelte';
	import HouseFilled from './icons/house-filled.svelte';
	import ServerFilled from './icons/server-filled.svelte';
	import NavButton from './nav-button.svelte';

	let { appearance, onAppearanceChange }: {
		appearance: AppearanceMode;
		onAppearanceChange: (mode: AppearanceMode) => void;
	} = $props();

	const pathname = $derived(page.url.pathname);
	const homeSelected = $derived(pathname === resolve('/'));
	const clientsSelected = $derived(pathname === resolve('/clients') || pathname.startsWith(`${resolve('/clients')}/`));
	const serversSelected = $derived(pathname === resolve('/servers') || pathname.startsWith(`${resolve('/servers')}/`));
	const modsSelected = $derived(pathname === resolve('/mods') || pathname.startsWith(`${resolve('/mods')}/`));
	const shadersSelected = $derived(pathname === resolve('/shaders') || pathname.startsWith(`${resolve('/shaders')}/`));
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
		<NavButton aria-label="首页" selected={homeSelected} onclick={() => goto(resolve('/'))}>{#if homeSelected}<HouseFilled />{:else}<House size={19} />{/if}</NavButton>
		<NavButton aria-label="客户端" selected={clientsSelected} onclick={() => goto(resolve('/clients'))}>
			{#if clientsSelected}<GamepadFilled />{:else}<Gamepad2 size={19} />{/if}
		</NavButton>
		<NavButton aria-label="服务端" selected={serversSelected} onclick={() => goto(resolve('/servers'))}>{#if serversSelected}<ServerFilled />{:else}<Server size={19} />{/if}</NavButton>
		<NavButton aria-label="模组" selected={modsSelected} onclick={() => goto(resolve('/mods'))}>{#if modsSelected}<BlocksFilled />{:else}<Blocks size={19} />{/if}</NavButton>
		<NavButton aria-label="光影" selected={shadersSelected} onclick={() => goto(resolve('/shaders'))}><Sparkles size={19} /></NavButton>
	</div>
	<div class="mt-auto flex flex-col items-center gap-2.5">
		<UpdateNavButton />
		<AccountMenu {appearance} {onAppearanceChange} />
	</div>
</nav>
