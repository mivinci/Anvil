import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { resolveLocale, isLocale, messages, locales, formatBytes, formatCount, formatDate, formatDateTime, plural } from '../src/lib/i18n/index.ts';

const bootstrap = readFileSync(new URL('../static/locale-bootstrap.js', import.meta.url), 'utf8');

test('saved preference, ordered system languages and fallback are deterministic', () => {
	assert.equal(resolveLocale('en', ['zh-CN']), 'en');
	assert.equal(resolveLocale('invalid', ['de-DE', 'en-US', 'zh']), 'en');
	assert.equal(resolveLocale(null, [], 'en-US'), 'en');
	assert.equal(resolveLocale(null, ['fr-FR'], 'en'), 'zh-CN');
	for (const language of ['zh', 'zh-Hans', 'zh-CN']) assert.equal(resolveLocale(null, [language]), 'zh-CN');
	assert.equal(resolveLocale(null), 'zh-CN');
	assert.equal(isLocale('en-US'), false);
	assert.equal(isLocale('en'), true);
});

test('first-frame bootstrap matches runtime policy, including blocked storage', () => {
	for (const saved of ['en', 'zh-CN', 'invalid', null]) {
		for (const languages of [[], ['de-DE', 'en-US'], ['zh-Hans'], ['fr']]) {
			for (const blocked of [false, true]) {
				const element = { lang: '', dataset: {} as Record<string, string> };
				runInNewContext(bootstrap, {
					document: { documentElement: element }, navigator: { languages, language: 'en-US' },
					localStorage: { getItem: () => { if (blocked) throw Error('blocked'); return saved; } },
				});
				assert.equal(element.lang, resolveLocale(blocked ? null : saved, languages, 'en-US'));
				assert.equal(element.dataset.localeStorageIssue, blocked ? 'read' : undefined);
			}
		}
	}
});

test('both dictionaries expose the same domains, keys and value types', () => {
	function shape(value: unknown): unknown {
		return typeof value === 'object' && value !== null
			? Object.fromEntries(Object.entries(value).map(([key, item]) => [key, shape(item)])) : typeof value;
	}
	assert.deepEqual(shape(messages.en), shape(messages['zh-CN']));
	assert.deepEqual(Object.keys(messages), [...locales]);
	assert.equal(messages.en.common.loading, 'Loading interface…');
});

test('byte formatting keeps decimal and binary units separate', () => {
	assert.equal(formatBytes(1024, 'en'), '1 KiB');
	assert.equal(formatBytes(1000, 'en', { base: 1000 }), '1 KB');
	assert.equal(formatBytes(1000, 'en'), '1,000 B');
	assert.equal(formatBytes(0, 'zh-CN'), '0 B');
	assert.equal(formatBytes(1536, 'en'), '1.5 KiB');
	assert.equal(formatBytes(-1536, 'en'), '-1.5 KiB');
});

test('Intl count, plural and date formatting respects explicit locale and time zone', () => {
	assert.equal(formatCount(12345, 'en'), '12,345');
	assert.equal(plural(1, 'en', { one: 'file', other: 'files' }), 'file');
	assert.equal(plural(0, 'en', { one: 'file', other: 'files' }), 'files');
	assert.equal(plural(1, 'zh-CN', { one: 'wrong', other: '个文件' }), '个文件');
	const date = new Date('2026-11-02T13:45:00Z');
	for (const locale of locales) {
		assert.equal(formatDate(date, locale, { timeZone: 'UTC' }),
			new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric', timeZone: 'UTC' }).format(date));
		assert.equal(formatDateTime(date, locale, { timeZone: 'UTC' }),
			new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', timeZone: 'UTC' }).format(date));
	}
});

test('shared shell messages interpolate complete sentences and preserve user names', () => {
	assert.equal(messages.en.nav.sidebarAction(true, 'mods'), 'Hide mod filters');
	assert.equal(messages['zh-CN'].nav.sidebarAction(false, 'clients'), '展开实例侧栏');
	assert.equal(messages.en.common.restartUpdate(null), 'Restart to update to the new version');
	assert.equal(messages.en.common.updateProgress(42), 'Preparing update 42%');
	assert.equal(messages.en.common.taskLabel('Downloading', null), 'Tasks: Downloading');
	assert.equal(messages.en.common.instanceTitle('海岛生存', 'Overview'), '海岛生存 · Overview · Anvil');
});

test('home editorial copy is bilingual and byte formatting retains download semantics', async () => {
	const { getAnvilNews } = await import('../src/lib/anvil-news.ts');
	const { formatBytes: downloadBytes } = await import('../src/lib/download-progress.ts');
	assert.deepEqual(getAnvilNews('en').map(item => [item.id, item.href, item.artwork]), getAnvilNews('zh-CN').map(item => [item.id, item.href, item.artwork]));
	assert.match(getAnvilNews('en')[0]!.title, /Your next adventure/);
	assert.equal(downloadBytes(1000, 'en'), '1.00 KB');
	assert.equal(downloadBytes(1024, 'en'), '1.02 KB');
	assert.equal(downloadBytes(10_000, 'en'), '10.0 KB');
	assert.equal(downloadBytes(0, 'en'), '0 B');
});

test('resource quantities use singular at one, plural at zero or many, and grouped counts', () => {
	assert.equal(messages.en.instance.fileProgress(1, 1), '1 / 1 file');
	assert.equal(messages.en.instance.fileProgress(0, 0), '0 / 0 files');
	assert.equal(messages.en.instance.fileProgress(1234, 2345), '1,234 / 2,345 files');
	assert.equal(messages.en.instance.count(1, 'shaders'), '1 shader pack');
	assert.equal(messages.en.instance.count(0, 'shaders'), '0 shader packs');
	assert.equal(messages.en.instance.count(1234, 'packs'), '1,234 resource packs');
	assert.equal(messages.en.instance.reportsCount(1, false), '1 report');
	assert.equal(messages.en.instance.reportsCount(2, false), '2 reports');
	assert.equal(messages.en.clients.otherActive(1234), '1,234 other active instances');
	assert.equal(messages['zh-CN'].instance.fileProgress(1, 1), '1 / 1 个文件');
});
