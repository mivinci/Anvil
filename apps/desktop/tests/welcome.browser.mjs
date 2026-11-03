// Opt-in integration smoke. Local fixture only; build Desktop first.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.ANVIL_PLAYWRIGHT_MODULE ?? 'playwright');
const build = fileURLToPath(new URL('../build/', import.meta.url));
const output = path.resolve(build, '../.svelte-kit/welcome-qa');
await mkdir(output, { recursive: true });
let existing = false, accountsPresent = false, readFailure = false;
const requests = [];
const server = createServer(async (req, res) => {
	const url = new URL(req.url, 'http://fixture');
	if (req.headers.authorization) {
		requests.push(`${req.method} ${url.pathname}`);
		if (readFailure && url.pathname === '/accounts') { res.writeHead(503, { 'Content-Type': 'application/json' }).end(JSON.stringify({ error: 'fixture read failure', code: 'internal' })); return; }
		const data = url.pathname === '/health' ? { status: 'ok', apiVersion: '1.0.0' }
			: url.pathname === '/clients' && existing ? [{ id: 'fixture', name: 'fixture', versionId: '26.3' }]
			: url.pathname === '/accounts' && accountsPresent ? [{ id: 'fixture-account', username: 'Fixture', selected: true }]
			: url.pathname === '/accounts/fixture-account/verify' ? { valid: true }
			: url.pathname === '/account-operations/active' ? { operation: null }
			: url.pathname === '/news' ? { items: [], stale: false, freshForMs: 300000, revision: 1, refreshError: null }
			: url.pathname === '/play-stats' ? { schemaVersion: 1, accounts: {} } : [];
		res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(data)); return;
	}
	try {
		const requested = path.resolve(build, `.${decodeURIComponent(url.pathname)}`);
		if (requested !== path.resolve(build) && !requested.startsWith(build)) { res.writeHead(403).end(); return; }
		let body, extension;
		try { body = await readFile(requested); extension = path.extname(requested); }
		catch { body = await readFile(path.join(build, 'index.html')); extension = '.html'; }
		res.writeHead(200, { 'Content-Type': ({ '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.svg': 'image/svg+xml' })[extension] ?? 'application/octet-stream' }).end(body);
	} catch { res.writeHead(500).end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
	browser = await chromium.launch({ channel: process.env.ANVIL_BROWSER_CHANNEL ?? 'msedge', headless: true });
	async function makeContext(options = {}) {
	const context = await browser.newContext({ viewport: options.viewport ?? { width: 900, height: 620 }, locale: options.locale ?? 'zh-CN' });
	await context.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
	await context.addInitScript(({ origin, options }) => {
		if (!sessionStorage.getItem('fixture-seeded')) {
			for (const [key, value] of Object.entries(options.seed ?? {})) localStorage.setItem(key, value);
			sessionStorage.setItem('fixture-seeded', 'yes');
		}
		if (options.blockRead) Object.defineProperty(window, 'localStorage', { get() { throw new DOMException('fixture denied', 'SecurityError'); } });
		else if (options.blockWrite) Storage.prototype.setItem = () => { throw new DOMException('fixture denied', 'SecurityError'); };
		window.isTauri = true;
		window.__TAURI_INTERNALS__ = { invoke: async command => command === 'shell_get_runtime_connection' ? { baseUrl: origin, token: 'fixture', apiVersion: '1.0.0' }
			: command === 'shell_get_runtime_status' ? { state: 'ready' }
			: command === 'shell_get_app_info' ? { appId: 'anvil.desktop', appVersion: 'fixture', shellApiVersion: '1.0.0' } : true };
	}, { origin, options });
		return context;
	}
	const context = await makeContext();
	const page = await context.newPage();
	page.setDefaultTimeout(10000);
	const errors = []; page.on('pageerror', error => errors.push(error.message));
	await page.goto(`${origin}/settings/general`);
	await page.getByRole('heading', { name: '欢迎使用 Anvil', exact: true }).waitFor();
	assert.equal(await page.getByRole('navigation').count(), 0);
	assert.equal(await page.locator('[data-download-tasks-trigger]').count(), 0);
	assert.equal(await page.evaluate(() => localStorage.getItem('anvil.welcome')), 'pending');
	assert.deepEqual(requests.filter(value => !['GET /health', 'GET /accounts', 'GET /clients'].includes(value)), []);
	await page.locator('#welcome-language').click();
	await page.getByRole('option', { name: 'English', exact: true }).click();
	await page.getByRole('heading', { name: 'Welcome to Anvil', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Dark', exact: true }).click();
	await page.screenshot({ path: path.join(output, 'welcome-en-dark.png') });
	await page.reload();
	await page.getByRole('heading', { name: 'Welcome to Anvil', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Get started', exact: true }).click();
	await page.waitForURL(`${origin}/`);
	await page.getByRole('navigation', { name: 'Main navigation', exact: true }).waitFor();
	assert.equal(await page.evaluate(() => localStorage.getItem('anvil.welcome')), 'complete');
	await page.reload();
	await page.getByRole('navigation', { name: 'Main navigation', exact: true }).waitFor();
	assert.equal(await page.locator('#welcome-title').count(), 0);
	await page.evaluate(() => localStorage.clear());
	existing = true;
	await page.reload();
	await page.getByRole('navigation').first().waitFor();
	assert.equal(await page.locator('#welcome-title').count(), 0);
	assert.equal(await page.evaluate(() => localStorage.getItem('anvil.welcome')), 'complete');
	await context.close();
	existing = false;
	for (const scenario of [
		{ name: 'preferences', seed: { 'anvil.locale': 'en' } },
		{ name: 'account', account: true },
		{ name: 'unknown-data', failure: true },
		{ name: 'present-with-failed-account-read', failure: true, installed: true },
		{ name: 'unreadable-storage', blockRead: true },
	]) {
		accountsPresent = !!scenario.account; readFailure = !!scenario.failure; existing = !!scenario.installed;
		const ctx = await makeContext(scenario);
		const tab = await ctx.newPage(); tab.on('pageerror', error => errors.push(error.message));
		await tab.goto(`${origin}/`);
		await tab.getByRole('navigation').first().waitFor();
		assert.equal(await tab.locator('#welcome-title').count(), 0, scenario.name);
		if (!scenario.blockRead) assert.equal(await tab.evaluate(() => localStorage.getItem('anvil.welcome')), scenario.failure && !scenario.installed ? null : 'complete', scenario.name);
		if (scenario.blockRead) await tab.getByRole('status').filter({ hasText: '暂时无法保存欢迎设置' }).waitFor();
		await ctx.close();
	}
	accountsPresent = false; readFailure = false; existing = false;
	const unwritable = await makeContext({ seed: { 'anvil.welcome': 'pending' }, blockWrite: true });
	const blocked = await unwritable.newPage(); blocked.on('pageerror', error => errors.push(error.message));
	await blocked.goto(`${origin}/`);
	await blocked.getByRole('heading', { name: '欢迎使用 Anvil', exact: true }).waitFor();
	await blocked.getByRole('button', { name: '开始使用', exact: true }).click();
	await blocked.getByRole('navigation').first().waitFor();
	await blocked.getByRole('status').filter({ hasText: '暂时无法保存欢迎设置' }).waitFor();
	await blocked.getByRole('button', { name: '关闭', exact: true }).click();
	assert.equal(await blocked.getByText('暂时无法保存欢迎设置', { exact: false }).count(), 0);
	assert.equal(await blocked.evaluate(() => localStorage.getItem('anvil.welcome')), 'pending');
	await unwritable.close();

	for (const locale of ['zh-CN', 'en']) for (const viewport of [{ width: 620, height: 420 }, { width: 1280, height: 800 }]) {
		const ctx = await makeContext({ locale, viewport });
		const tab = await ctx.newPage(); tab.on('pageerror', error => errors.push(error.message));
		await tab.goto(`${origin}/`);
		await tab.locator('#welcome-title').waitFor();
		await tab.locator('#welcome-language').focus();
		await tab.keyboard.press('Enter');
		assert.equal(await tab.getByRole('listbox').count(), 1);
		await tab.keyboard.press('Escape');
		assert.equal(await tab.locator('#welcome-language').evaluate(element => element === document.activeElement), true);
		await tab.keyboard.press('Enter');
		await tab.keyboard.press('End');
		await tab.keyboard.press('Enter');
		await tab.getByRole('heading', { name: 'Welcome to Anvil', exact: true }).waitFor();
		await tab.locator('#welcome-language').click();
		await tab.getByRole('option', { name: locale === 'en' ? 'English' : '简体中文', exact: true }).click();
		for (const mode of locale === 'en' ? ['System', 'Light', 'Dark'] : ['跟随系统', '浅色', '深色']) {
			const button = tab.getByRole('button', { name: mode, exact: true });
			await button.click(); assert.equal(await button.getAttribute('aria-pressed'), 'true');
			for (const theme of locale === 'en' ? ['Shulker Purple', 'Enderman Black', 'Creeper Green', 'Allay Blue'] : ['潜影紫', '末影黑', '苦力怕绿', '悦灵蓝']) {
				const preset = tab.getByRole('button', { name: theme, exact: true });
				await preset.click(); assert.equal(await preset.getAttribute('aria-pressed'), 'true');
				assert.equal(await tab.evaluate(() => document.documentElement.scrollWidth <= innerWidth && document.querySelector('main').scrollWidth <= document.querySelector('main').clientWidth), true);
			}
		}
		const start = tab.getByRole('button', { name: locale === 'en' ? 'Get started' : '开始使用', exact: true });
		await start.scrollIntoViewIfNeeded();
		const bounds = await start.boundingBox();
		assert.ok(bounds.y >= 32 && bounds.y + bounds.height <= viewport.height + 1);
		assert.equal(await start.evaluate(element => element.scrollWidth <= element.clientWidth), true);
		await tab.screenshot({ path: path.join(output, `welcome-${locale}-${viewport.width}.png`) });
		await ctx.close();
	}
	assert.deepEqual(errors, []);
	assert.deepEqual(requests.filter(value => !value.startsWith('GET ') && value !== 'POST /accounts/fixture-account/verify'), []);
	console.log('PASS: welcome lifecycle, existing preferences/accounts/instances, unknown data, storage failures, keyboard selection/Escape, and 48 bilingual size/mode/theme combinations');
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
