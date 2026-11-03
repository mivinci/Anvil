import assert from 'node:assert/strict';
import { test, after } from 'node:test';
import { readFile, writeFile, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';
import { compileLocale } from './compile-locale.mjs';
const root = fileURLToPath(new URL('../../../', import.meta.url));
await mkdir(resolve(root, 'target'), { recursive: true });
const directory = await mkdtemp(resolve(root, 'target/client-flow-i18n-'));
after(() => rm(directory, { recursive: true, force: true }));
const localeUrl = await compileLocale(root, directory);
const { localeState } = await import(localeUrl);
const adapter = resolve(directory, 'runtime.mjs');
await writeFile(adapter, `
export let resourceCalls = 0;
export let accountMode = 'ready', loginCalls = 0, accountReads = 0, statsClears = 0;
export const setAccountMode = value => { accountMode = value; };
export const listAccounts = async () => { accountReads++; if (accountMode === 'readFail') throw Error('fixture account read detail'); return []; };
export const logoutAccount = async () => { throw new RuntimeRequestError('fixture sign-out detail', 409); };
export const selectAccount = async () => { throw new RuntimeRequestError('fixture switch detail', 409); };
export const verifyAccount = async () => { throw Error('unexpected verification'); };
export const getAccountLoginConfiguration = async () => ({ enabled: true, unavailableReason: null });
export const startAccountLogin = async () => { loginCalls++; return { id: 1, state: accountMode === 'cancelledLegacy' ? 'cancelled' : 'failed', stage: 'waiting_for_approval', error: 'expired fixture detail', errorCode: accountMode === 'cancelledLegacy' ? undefined : 'authentication_failed', authorization: null, account: null }; };
export const getActiveAccountLogin = async () => ({ operation: null });
export const getAccountLoginOperation = async () => { throw Error('unexpected polling'); };
export const cancelAccountLogin = async () => { throw Error('unexpected cancellation'); };
export const getPlayStats = async () => ({ schemaVersion: 1, accounts: {} });
export const clearPlayStats = async () => { statsClears++; throw new RuntimeRequestError('fixture uncertain clear detail', 500); };
export const listResourceOperations = async () => [];
export const startResourceInstall = async () => { resourceCalls++; throw new RuntimeRequestError('fixture resource conflict', 409, 'plan_expired'); };
export const cancelResourceOperation = async () => { throw Error('unexpected resource cancellation'); };
export class RuntimeRequestError extends Error { constructor(message, status, code = null) { super(message); this.status = status; this.code = code; } }
export const listInstalledClients = async () => [];
export const deleteClient = async () => { throw new RuntimeRequestError('fixture conflict detail', 409); };
export const getClientDetails = async () => ({});
export const listClientLaunches = async () => [];
export const startClientLaunch = async () => { throw Error('unexpected launch'); };
export const stopClientLaunch = async () => { throw Error('unexpected stop'); };
`);
const transpile = text => ts.transpileModule(text, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
await writeFile(resolve(directory, 'recent.mjs'), transpile(await readFile(resolve(root, 'apps/desktop/src/lib/recent-plays.ts'), 'utf8')));
async function module(name) {
	const js = transpile(await readFile(resolve(root, `apps/desktop/src/lib/${name}.svelte.ts`), 'utf8'));
	const compiled = compileModule(js, { filename: `${name}.svelte.js`, generate: 'client' }).js.code
		.replace(/import \{ createContext \} from ['"]svelte['"];?/, 'const createContext = () => [() => null, value => value];')
		.replace(/from ['"]\.\/locale\.svelte\.ts['"]/g, `from ${JSON.stringify(localeUrl)}`)
		.replace(/from ['"]\.\/runtime-errors\.ts['"]/g, "from './runtime-errors.mjs'")
  .replace(/from ['"]\.\/runtime['"]/g, `from ${JSON.stringify(pathToFileURL(adapter).href)}`)
		.replace(/from ['"]\.\/recent-plays['"]/g, "from './recent.mjs'");
	const target = resolve(directory, `${name}.mjs`);
	await writeFile(target, compiled);
	return import(pathToFileURL(target).href);
}
const { AccountState } = await module('account-state');
const { PlayStatsState } = await module('play-stats-state');
const { ResourceInstallState } = await module('resource-install-state');
const resourceAdapter = await import(pathToFileURL(adapter).href);
const { launchLabel } = await module('client-launch-state');
const { ClientLibraryState } = await module('client-library-state');
const { ClientRemovalState } = await module('client-removal-state');

test('launch labels translate by phase and never expose raw server errors as primary text', () => {
	try {
		localeState.setLocale('en');
		assert.equal(launchLabel({ phase: 'preparing' }), 'Preparing to launch…');
		assert.equal(launchLabel({ phase: 'failed', error: 'fixture raw detail' }), 'Launch failed');
		assert.equal(launchLabel({ phase: 'failed', error: 'wording changed', errorCode: 'java_required' }), 'A matching managed Java is missing. Install it to launch the game.');
		assert.equal(launchLabel({ phase: 'failed', error: 'wording changed', errorCode: 'future_error' }), 'The operation failed. Please try again.');
		localeState.setLocale('zh-CN');
		assert.equal(launchLabel({ phase: 'preparing' }), '正在准备启动…');
	} finally { localeState.setLocale('zh-CN'); }
});

test('existing removal receipts and failures switch language while retaining technical detail', async () => {
	const library = new ClientLibraryState();
	const removal = new ClientRemovalState(() => assert.fail('rejected deletion cannot complete'));
	try {
		localeState.setLocale('en');
		library.acceptRemoved('海岛生存', true);
		assert.match(library.removalNotice, /海岛生存.*removed/);
		assert.equal(await removal.remove('fixture'), false);
		assert.equal(removal.uncertain, false);
		assert.equal(removal.error, 'Deletion did not finish. Please retry.');
		assert.equal(removal.errorDetail, 'fixture conflict detail');
		localeState.setLocale('zh-CN');
		assert.match(library.removalNotice, /部分文件/);
		assert.match(removal.error, /删除未完成/);
	} finally { localeState.setLocale('zh-CN'); library.dispose(); removal.dispose(); }
});

test('resource rejection and task phases translate without resubmitting or losing raw detail', async () => {
	const state = new ResourceInstallState();
	try {
		await state.sync();
		localeState.setLocale('en');
		const before = resourceAdapter.resourceCalls;
		assert.equal(await state.start('海岛生存', 12), false);
		assert.equal(state.uncertain, false);
		assert.equal(state.intent, null);
		assert.match(state.error, /plan expired/);
		assert.equal(state.errorDetail, 'fixture resource conflict');
		state.operation = { id: 12, name: '海岛生存', kind: 'shader', state: 'running', cancelRequested: false, progress: { stage: 'downloading' } };
		assert.equal(state.label, 'Downloading and verifying shader files…');
		localeState.setLocale('zh-CN');
		assert.match(state.error, /计划已过期/);
		assert.equal(state.label, '正在下载并校验光影…');
		assert.equal(resourceAdapter.resourceCalls, before + 1);
		assert.equal(state.errorDetail, 'fixture resource conflict');
	} finally { state.dispose(); localeState.setLocale('zh-CN'); }
});

test('retained account action errors and expired sign-in switch language without new requests', async () => {
	const state = new AccountState();
	try {
		localeState.setLocale('en');
		resourceAdapter.setAccountMode('readFail');
		assert.equal(await state.refreshAccounts(), false);
		assert.match(state.error.message, /Could not load accounts/);
		assert.equal(state.error.detail, 'fixture account read detail');
		const reads = resourceAdapter.accountReads;
		localeState.setLocale('zh-CN');
		assert.match(state.error.message, /无法读取账户/);
		assert.equal(resourceAdapter.accountReads, reads);
		resourceAdapter.setAccountMode('ready');
		localeState.setLocale('en');
		assert.equal(await state.startLogin(), true);
		assert.match(state.loginError, /Account verification failed/);
		assert.equal(state.loginErrorDetail, 'expired fixture detail');
		const starts = resourceAdapter.loginCalls;
		localeState.setLocale('zh-CN');
		assert.match(state.loginError, /账户验证未完成/);
		assert.equal(resourceAdapter.loginCalls, starts);
		assert.equal(state.login.id, 1);
		let offlineLaunches = 0;
		state.openLoginDialog(() => { offlineLaunches++; });
		state.useOfflineMode();
		assert.equal(state.offlineMode, true);
		assert.equal(state.loginDialogOpen, false);
		assert.equal(offlineLaunches, 1);
		assert.equal(resourceAdapter.loginCalls, starts);
		resourceAdapter.setAccountMode('cancelledLegacy');
		localeState.setLocale('en');
		const legacy = new AccountState();
		try {
			assert.equal(await legacy.startLogin(), true);
			assert.match(legacy.loginError, /Review the error details to confirm cleanup/);
			assert.equal(legacy.loginErrorDetail, 'expired fixture detail');
		} finally { legacy.dispose(); }
	} finally { state.dispose(); resourceAdapter.setAccountMode('ready'); localeState.setLocale('zh-CN'); }
});

test('uncertain history clearing stays blocked while its message changes language', async () => {
	const stats = new PlayStatsState();
	try {
		localeState.setLocale('en');
		assert.equal(await stats.clear({ kind: 'all' }), false);
		assert.equal(stats.clearUncertain, true);
		assert.match(stats.clearError, /Clearing is unconfirmed/);
		assert.equal(stats.clearErrorDetail, 'fixture uncertain clear detail');
		const calls = resourceAdapter.statsClears;
		localeState.setLocale('zh-CN');
		assert.match(stats.clearError, /清除结果尚未确认/);
		assert.equal(await stats.clear({ kind: 'all' }), false);
		assert.equal(resourceAdapter.statsClears, calls);
		assert.equal(await stats.refresh(), true);
		assert.equal(stats.clearError, null);
		assert.equal(stats.clearErrorDetail, null);
		assert.equal(stats.clearUncertain, false);
	} finally { stats.dispose(); localeState.setLocale('zh-CN'); }
});
