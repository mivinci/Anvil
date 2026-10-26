import assert from 'node:assert/strict';
import test from 'node:test';
import { defaultTheme, readTheme, themeStyle, themePresets, customThemeFrom } from '../src/lib/appearance.ts';

test('preset rendering ignores custom drafts, including old accent overrides', () => {
	const preferences = defaultTheme();
	preferences.light.accent = '#ff0000';
	preferences.light.background = '#123456';
	assert.equal(themeStyle(preferences, 'light'), themeStyle(defaultTheme(), 'light'));
});

test('custom palettes persist independently while a preset is selected', () => {
	const preferences = defaultTheme();
	preferences.dark.card = '#345678';
	const restored = readTheme(JSON.stringify(preferences));
	assert.equal(restored.theme, 'default');
	assert.equal(restored.dark.card, '#345678');
	restored.theme = 'custom';
	assert.match(themeStyle(restored, 'dark'), /--card:#345678(?:;|$)/);
});

test('older custom palettes preserve their shared background and reject invalid colors', () => {
	const restored = readTheme(JSON.stringify({ theme: 'custom', light: { background: '#123456', accent: 'invalid' } }));
	assert.equal(restored.light.card, '#123456');
	assert.equal(restored.light.accent, defaultTheme().light.accent);
	assert.equal(restored.dark.card, defaultTheme().dark.card);
});


test('all four presets restore by id and copy both modes without changing the rendered colors', () => {
	assert.equal(themePresets.length, 4);
	for (const preset of themePresets) {
		const preferences = readTheme(JSON.stringify({ theme: preset.id }));
		assert.equal(preferences.theme, preset.id);
		const custom = customThemeFrom(preset.id);
		for (const mode of ['light', 'dark'] as const) {
			assert.equal(themeStyle(preferences, mode), themeStyle(custom, mode));
			assert.notEqual(custom[mode], preset[mode]);
		}
		assert.equal(custom.base, preset.id);
	}
});

function luminance(color: string) {
	const [r = 0, g = 0, b = 0] = [1, 3, 5].map(offset => {
		const value = parseInt(color.slice(offset, offset + 2), 16) / 255;
		return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a: string, b: string) {
	const values = [luminance(a), luminance(b)];
	return (Math.max(...values) + 0.05) / (Math.min(...values) + 0.05);
}

test('all preset body and button text meet 4.5:1 contrast in both modes', () => {
	for (const preset of themePresets) {
		for (const mode of ['light', 'dark'] as const) {
			const palette = preset[mode];
			assert.ok(contrast(palette.background, palette.foreground) >= 4.5, `${preset.name} ${mode} background`);
			assert.ok(contrast(palette.card, palette.foreground) >= 4.5, `${preset.name} ${mode} card`);
			const style = Object.fromEntries(themeStyle(customThemeFrom(preset.id), mode).split(';').map(field => field.split(':')));
			assert.ok(contrast(style['--primary']!, style['--primary-foreground']!) >= 4.5, `${preset.name} ${mode} button`);
		}
	}
});
