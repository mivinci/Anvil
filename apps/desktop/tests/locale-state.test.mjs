import assert from 'node:assert/strict';
import { test, after } from 'node:test';
import { readFile, writeFile, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';

const root = fileURLToPath(new URL('../../../', import.meta.url));
await mkdir(resolve(root, 'target'), { recursive: true });
const directory = await mkdtemp(resolve(root, 'target/locale-state-test-'));
after(() => rm(directory, { recursive: true, force: true }));
const source = await readFile(resolve(root, 'apps/desktop/src/lib/locale.svelte.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const compiled = compileModule(js, { filename: 'locale.svelte.js', generate: 'client' }).js.code
	.replace("'./i18n/index.ts'", JSON.stringify(pathToFileURL(resolve(root, 'apps/desktop/src/lib/i18n/index.ts')).href));
const file = resolve(directory, 'state.mjs');
await writeFile(file, compiled);
const { LocaleState } = await import(pathToFileURL(file).href);

function fixture(saved = null, languages = ['en-US']) {
	const values = new Map(saved === null ? [] : [['anvil.locale', saved]]);
	const document = { lang: '' };
	return { values, document, environment: {
		storage: { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value), removeItem: key => values.delete(key) },
		languages, language: 'zh-CN', setDocumentLanguage: lang => { document.lang = lang; },
	} };
}

test('initializes from saved preference and switches language without reload', () => {
	const { environment, document, values } = fixture('zh-CN');
	const state = new LocaleState(environment);
	assert.equal(state.current, 'zh-CN');
	assert.equal(state.preference, 'zh-CN');
	state.setLocale('en');
	assert.equal(document.lang, 'en');
	assert.equal(values.get('anvil.locale'), 'en');
	assert.equal(state.messages.common.cancel, 'Cancel');
	assert.equal(state.storageMessage, null);
	state.setLocale('system');
	assert.equal(state.current, 'en');
	assert.equal(state.preference, 'system');
	assert.equal(values.has('anvil.locale'), false);
});

test('blocked reads fall back to system with an explicit localized status', () => {
	const { environment } = fixture();
	environment.storage.getItem = () => { throw Error('denied'); };
	const state = new LocaleState(environment);
	assert.equal(state.current, 'en');
	assert.equal(state.storageIssue, 'read');
	assert.match(state.storageMessage, /Could not read/);
});

test('failed writes retain current-window language and recover on successful save', () => {
	const { environment, document } = fixture();
	const save = environment.storage.setItem;
	environment.storage.setItem = () => { throw Error('quota'); };
	const state = new LocaleState(environment);
	state.setLocale('zh-CN');
	assert.equal(state.current, 'zh-CN');
	assert.equal(document.lang, 'zh-CN');
	assert.equal(state.storageIssue, 'write');
	assert.match(state.storageMessage, /当前窗口/);
	environment.storage.setItem = save;
	state.setLocale('en');
	assert.equal(state.storageIssue, null);
});

test('failed removal is visible and does not undo system preference for this window', () => {
	const { environment } = fixture('zh-CN');
	environment.storage.removeItem = () => { throw Error('denied'); };
	const state = new LocaleState(environment);
	state.setLocale('system');
	assert.equal(state.current, 'en');
	assert.equal(state.storageIssue, 'write');
});

test('invalid persisted values fall back and unsupported selections never write', () => {
	const { environment, values } = fixture('unsupported', []);
	const state = new LocaleState(environment);
	assert.equal(state.current, 'zh-CN');
	assert.equal(state.preference, 'system');
	assert.throws(() => state.setLocale('fr'), TypeError);
	assert.equal(values.get('anvil.locale'), 'unsupported');
});
