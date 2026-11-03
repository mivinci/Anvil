import assert from 'node:assert/strict';
import { test } from 'node:test';
import { applyUiScale, readUiScale, uiScaleStorageKey, uiScales } from '../src/lib/ui-scale.ts';

test('text scale accepts only the supported presets', () => {
	assert.equal(uiScaleStorageKey, 'anvil.ui-scale');
	assert.deepEqual(uiScales, ['default', 'large', 'larger']);
	assert.equal(readUiScale('default'), 'default');
	assert.equal(readUiScale('large'), 'large');
	assert.equal(readUiScale('larger'), 'larger');
	for (const value of [null, '', 'small', 'LARGE', 1, {}]) assert.equal(readUiScale(value), 'default');
});

test('applying a text scale writes the root data attribute', () => {
	const root = { dataset: {} as Record<string, string> } as HTMLElement;
	applyUiScale('large', root);
	assert.equal(root.dataset.uiScale, 'large');
	applyUiScale('default', root);
	assert.equal(root.dataset.uiScale, 'default');
});
