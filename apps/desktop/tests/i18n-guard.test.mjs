import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { compareMessages, inspectSource, inspectTree } from '../../../scripts/lib/desktop-i18n-guard.mjs';

const messages = { 'zh-CN': { common: { loading: '加载中' } }, en: { common: { loading: 'Loading' } } };
const startup = Object.entries(messages).map(([locale, dictionary]) => `<span lang="${locale}" data-startup-lang="${locale}">${dictionary.common.loading}</span>`).join('');

test('guard checks nested keys, value types, array lengths and interpolation arity', () => {
	assert.deepEqual(compareMessages({ nested: { text: '源', value: x => x } }, { nested: { text: 'Source', value: x => x } }), []);
	assert.match(compareMessages({ a: { b: '' } }, { a: {} }).join(), /missing translation/);
	assert.match(compareMessages({}, { extra: '' }).join(), /unexpected translation/);
	assert.match(compareMessages({ a: '' }, { a: () => '' }).join(), /type differs/);
	assert.match(compareMessages({ a: x => x }, { a: () => '' }).join(), /arity differs/);
	assert.match(compareMessages(['a'], []).join(), /array length differs/);
});

test('guard rejects visible and accessibility text with useful source locations', () => {
	assert.deepEqual(inspectSource('page.svelte', '<button>Import</button>', messages), []);
	assert.match(inspectSource('page.svelte', '<button>\n导入</button>', messages).join(), /page.svelte:2:1/);
	assert.match(inspectSource('page.svelte', '<input aria-label="搜索" />', messages).join(), /Chinese text/);
});

test('startup exemption requires exact bilingual dictionary copies and remains narrow', () => {
	assert.deepEqual(inspectSource('app.html', startup, messages), []);
	for (const source of [startup.replace('加载中', '等待'), startup + startup, startup.replace('Loading', 'Wait')]) {
		assert.match(inspectSource('app.html', source, messages).join(), /startup copy/);
	}
	assert.match(inspectSource('app.html', startup + '<p>硬编码</p>', messages).join(), /Chinese text/);
	assert.match(inspectSource('other.html', startup, messages).join(), /Chinese text/);
});

test('recursive source scan excludes only the dictionary directory', async () => {
	const root = await mkdtemp(path.join(os.tmpdir(), 'anvil-i18n-guard-'));
	try {
		await mkdir(path.join(root, 'lib/i18n'), { recursive: true });
		await mkdir(path.join(root, 'routes'));
		await writeFile(path.join(root, 'lib/i18n/zh.ts'), '字典');
		await writeFile(path.join(root, 'routes/page.svelte'), '<h1>遗漏</h1>');
		const issues = await inspectTree(root, messages);
		assert.equal(issues.length, 1);
		assert.match(issues[0], /routes\/page.svelte/);
	} finally { await rm(root, { recursive: true, force: true }); }
});
