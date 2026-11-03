import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
const script = await readFile(new URL('../bootstrap/__anvil_shell__/recovery.js', import.meta.url), 'utf8');
const html = await readFile(new URL('../bootstrap/index.html', import.meta.url), 'utf8');
function environment(languages, language) {
	const events = new Map(), clicks = new Map();
	let reloads = 0, storageReads = 0;
	const document = {
		documentElement: { lang: 'zh-CN', dataset: {} }, title: '',
		addEventListener: (name, callback, options) => events.set(name, { callback, options }),
		getElementById: id => id === 'retry' ? { addEventListener: (name, callback) => clicks.set(name, callback) } : null,
	};
	const context = { navigator: { languages, language }, document, location: { reload: () => { reloads++; } } };
	Object.defineProperty(context, 'localStorage', { get() { storageReads++; throw Error('App storage must not be accessed'); } });
	runInNewContext(script, context);
	return { document, events, clicks, get reloads() { return reloads; }, get storageReads() { return storageReads; } };
}
test('recovery uses ordered system languages with zh-CN fallback and never reads App storage', () => {
	for (const [languages, language, expected] of [
		[['en-US'], 'zh-CN', 'en'], [['fr', 'en-GB', 'zh'], 'zh-CN', 'en'],
		[['zh-Hans', 'en'], 'en', 'zh-CN'], [['zh-TW'], 'en', 'zh-CN'],
		[[], 'en-AU', 'en'], [[], 'zh', 'zh-CN'], [[], 'de', 'zh-CN'],
		[['fr'], 'en-US', 'zh-CN'], [['de', 'fr'], 'en', 'zh-CN'],
	]) {
		const env = environment(languages, language);
		assert.equal(env.document.documentElement.lang, expected);
		assert.equal(env.document.documentElement.dataset.recoveryLocale, expected);
		assert.equal(env.document.title, expected === 'en' ? 'Anvil — Recovery' : 'Anvil — 恢复');
		assert.equal(env.storageReads, 0);
	}
});
test('the external script binds retry after DOM creation and reloads only on an explicit click', () => {
	const env = environment(['en'], 'en');
	assert.equal(env.reloads, 0);
	assert.equal(env.clicks.size, 0);
	assert.equal(env.events.get('DOMContentLoaded').options.once, true);
	env.events.get('DOMContentLoaded').callback();
	assert.equal(env.reloads, 0);
	env.clicks.get('click')();
	assert.equal(env.reloads, 1);
});
test('recovery is self-contained under script-src self with static bilingual fallback', () => {
	assert.match(html, /<meta http-equiv="Content-Security-Policy"[^>]+script-src 'self';/);
	assert.doesNotMatch(html, /script-src[^;]+unsafe-inline|\bon\w+\s*=/i);
	assert.deepEqual([...html.matchAll(/<script\b[^>]*src="([^"]+)"[^>]*><\/script>/g)].map(value => value[1]), ['/__anvil_shell__/recovery.js']);
	assert.match(html, /lang="en" data-language="en">Retry/);
	assert.match(html, /lang="zh-CN" data-language="zh-CN">重试/);
	assert.match(html, /missing, damaged or incompatible/);
});
