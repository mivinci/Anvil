import { createContext } from 'svelte';
import type { AppearanceMode } from './components/appearance-menu.svelte';

export type ColorMode = 'light' | 'dark';
export type Palette = { background: string; foreground: string; accent: string };
export type ThemePreferences = { theme: 'default' | 'custom'; light: Palette; dark: Palette };
export const defaultTheme = (): ThemePreferences => ({
	theme: 'default',
	light: { background: '#f8f8f7', foreground: '#2d2e30', accent: '#827099' },
	dark: { background: '#191a1b', foreground: '#e9e9e7', accent: '#b5a0ce' },
});
export const isColor = (value: unknown): value is string => typeof value === 'string' && /^#[\da-f]{6}$/i.test(value);
export function readTheme(value: string | null): ThemePreferences {
	const fallback = defaultTheme();
	try {
		const saved = JSON.parse(value ?? 'null');
		if (saved?.theme === 'custom') fallback.theme = 'custom';
		for (const mode of ['light', 'dark'] as const) {
			for (const key of ['background', 'foreground', 'accent'] as const) {
				if (isColor(saved?.[mode]?.[key])) fallback[mode][key] = saved[mode][key];
			}
		}
	} catch { /* Invalid saved settings use the built-in palette. */ }
	return fallback;
}
function channels(hex: string) { return [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16)); }
function blend(base: string, ink: string, amount: number): string {
	const a = channels(base), b = channels(ink);
	return `#${a.map((value, index) => Math.round(value * (1 - amount) + (b[index] ?? 0) * amount).toString(16).padStart(2, '0')).join('')}`;
}
function luminance(color: string): number {
	const [r = 0, g = 0, b = 0] = channels(color).map((channel) => {
		const value = channel / 255;
		return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function buttonColors(color: string): { background: string; foreground: string } {
	const backgroundLuminance = luminance(color);
	const contrast = (background: string, foreground: string) => {
		const a = luminance(background), b = luminance(foreground);
		return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
	};
	const whiteContrast = 1.05 / (backgroundLuminance + 0.05);
	const foreground = whiteContrast >= contrast(color, '#161616') ? '#ffffff' : '#161616';
	const shade = foreground === '#ffffff' ? '#000000' : '#ffffff';
	// Preserve the chosen ink and adjust middle-tone surfaces minimally instead
	// of abruptly switching to pure black at the contrast threshold.
	for (let percent = 0; percent <= 100; percent++) {
		const background = blend(color, shade, percent / 100);
		if (contrast(background, foreground) >= 4.5) return { background, foreground };
	}
	return { background: shade, foreground };
}
export function themeStyle(preferences: ThemePreferences, mode: ColorMode): string {
	const palette = preferences[mode];
	const accent = palette.accent;
	const button = buttonColors(accent);
	const variables: Record<string, string> = {
		primary: button.background, 'primary-foreground': button.foreground, ring: accent,
		'chrome-selected': `color-mix(in srgb, ${accent} 18%, transparent)`,
		'accent-foreground': blend(mode === 'dark' ? '#ffffff' : '#000000', accent, 0.7),
		accent: `color-mix(in srgb, ${accent} 15%, ${preferences.theme === 'custom' ? palette.background : 'var(--background)'})`,
	};
	if (preferences.theme === 'custom') {
		const { background, foreground } = palette;
		Object.assign(variables, {
			background, foreground, 'main-pane': background, card: background,
			'card-foreground': foreground, popover: blend(background, foreground, 0.04),
			'popover-foreground': foreground, secondary: blend(background, foreground, 0.06),
			'secondary-foreground': foreground, muted: blend(background, foreground, 0.06),
			'muted-foreground': blend(background, foreground, 0.62),
			'chrome-muted': blend(background, foreground, 0.62),
			'chrome-subtle': blend(background, foreground, 0.48),
			border: `color-mix(in srgb, ${foreground} 12%, transparent)`,
			input: `color-mix(in srgb, ${foreground} 16%, transparent)`,
			'chrome-control-border': `color-mix(in srgb, ${foreground} 12%, transparent)`,
			'chrome-hover': `color-mix(in srgb, ${foreground} 7%, transparent)`,
		});
	}
	return Object.entries(variables).map(([key, value]) => `--${key}:${value}`).join(';');
}
export interface AppearanceSettings {
	readonly mode: AppearanceMode;
	readonly colors: ThemePreferences;
	readonly resolvedMode: ColorMode;
	setMode: (mode: AppearanceMode) => void;
	setTheme: (theme: ThemePreferences['theme']) => void;
	setColor: (mode: ColorMode, key: keyof Palette, value: string) => void;
	resetColors: () => void;
	readonly saveError: string | null;
}
export const [getAppearance, setAppearance] = createContext<AppearanceSettings>();
