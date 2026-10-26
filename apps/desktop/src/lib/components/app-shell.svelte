<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { PageSidebarState, setPageSidebarState } from '../page-sidebar-state.svelte.js';
	setPageSidebarState(new PageSidebarState());
	import AppTitlebar from './app-titlebar.svelte';
	import AppStartup from './app-startup.svelte';
	import OverlayScrollbars from './overlay-scrollbars.svelte';
	import ClientInstallProvider from './clients/client-install-provider.svelte';
	import AccountProvider from './account-provider.svelte';
	import AppNavigation from './app-navigation.svelte';
	import { setAppearance as provideAppearance, defaultTheme, customThemeFrom, readTheme, themeStyle, isColor, type ColorMode, type Palette, type ThemePreferences } from '../appearance';
	import type { AppearanceMode } from './appearance-menu.svelte';

	let { children }: { children: Snippet } = $props();
	let appearance = $state<AppearanceMode>('system');
	let colors = $state(defaultTheme());
	let systemDark = $state(false);
	let saveError = $state<string | null>(null);
	const resolvedMode = $derived(appearance === 'system' ? (systemDark ? 'dark' : 'light') : appearance);
	provideAppearance({
		get mode() { return appearance; }, get colors() { return colors; },
		get resolvedMode() { return resolvedMode; }, get saveError() { return saveError; },
		setMode: setAppearance, setTheme, setColor,
		customizeTheme: () => { colors = customThemeFrom(colors.theme === 'custom' ? colors.base : colors.theme); saveColors(); },
		resetColors: () => { colors = { ...customThemeFrom(colors.base), theme: colors.theme }; saveColors(); },
	});
	function saveColors() {
		try { window.localStorage.setItem('anvil.theme', JSON.stringify(colors)); saveError = null; }
		catch { saveError = '无法保存配色，本次修改仅在当前窗口生效。'; }
	}
	function setTheme(theme: ThemePreferences['theme']) { colors.theme = theme; saveColors(); }
	function setColor(mode: ColorMode, key: keyof Palette, value: string) {
		if (colors.theme !== 'custom' || !isColor(value)) return;
		colors[mode][key] = value;
		saveColors();
	}

	onMount(() => {
		const media = window.matchMedia('(prefers-color-scheme: dark)');
		systemDark = media.matches;
		const onSystemChange = (event: MediaQueryListEvent) => { systemDark = event.matches; };
		media.addEventListener('change', onSystemChange);
		let savedAppearance: string | null = null;
		try {
			savedAppearance = window.localStorage.getItem('anvil.appearance');
			colors = readTheme(window.localStorage.getItem('anvil.theme'));
		} catch { saveError = '无法读取本机外观设置。'; }
		if (savedAppearance === 'system' || savedAppearance === 'light' || savedAppearance === 'dark') {
			appearance = savedAppearance;
		}
		syncNativeAppearance(appearance);
		return () => media.removeEventListener('change', onSystemChange);
	});

	function syncNativeAppearance(mode: AppearanceMode) {
		if (!isTauri()) return;
		void getCurrentWindow().setTheme(mode === 'system' ? null : mode).catch((error) => {
			console.error('Failed to synchronize native window appearance:', error);
		});
	}

	function setAppearance(mode: AppearanceMode) {
		appearance = mode;
		try { window.localStorage.setItem('anvil.appearance', mode); saveError = null; }
		catch { saveError = '无法保存外观模式，本次修改仅在当前窗口生效。'; }
		syncNativeAppearance(mode);
	}
</script>

<div
	class={`app-window fixed inset-0 flex min-h-0 w-full flex-col overflow-hidden overscroll-none rounded-md-[18px] bg-transparent ${appearance === 'light' ? 'scheme-light' : appearance === 'dark' ? 'scheme-dark' : 'scheme-light-dark'}`}
	data-theme={appearance}
	style={themeStyle(colors, resolvedMode)}
>
	<AppStartup>
	<AccountProvider>
	<ClientInstallProvider>
		<AppTitlebar />

		<div class="grid min-h-0 flex-1 grid-cols-[52px_minmax(0,1fr)]">
			<AppNavigation {appearance} onAppearanceChange={setAppearance} />

			<main class="mb-1 mr-1 flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[12px] bg-main-pane text-foreground" aria-label="主内容区域">
				{@render children()}
			</main>
		</div>
	</ClientInstallProvider>
	</AccountProvider>
	</AppStartup>
	<OverlayScrollbars />
</div>
