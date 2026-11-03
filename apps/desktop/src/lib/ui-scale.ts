export const uiScaleStorageKey = 'anvil.ui-scale';
export const uiScales = ['default', 'large', 'larger'] as const;
export type UiScale = (typeof uiScales)[number];

export function isUiScale(value: unknown): value is UiScale {
	return typeof value === 'string' && (uiScales as readonly string[]).includes(value);
}

export function readUiScale(value: unknown): UiScale {
	return isUiScale(value) ? value : 'default';
}

export function applyUiScale(scale: UiScale, root: HTMLElement = document.documentElement): void {
	root.dataset.uiScale = scale;
}
