// Opt-in offline browser acceptance; build first. No real Runtime, accounts or downloads.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.ANVIL_PLAYWRIGHT_MODULE ?? 'playwright');
const build = path.resolve(fileURLToPath(new URL('../build/', import.meta.url)));
const output = path.resolve(build, '../.svelte-kit/settings-language-qa');
await mkdir(output, { recursive: true });
let clients = [];
const catalog = { stale: false, freshForMs: 300000, revision: 1, refreshError: null };
const project = { id: 'abcdefgh', name: 'Fixture 模组原名', summary: '第三方摘要保持原文', authors: [{ name: 'Fixture author' }], links: { websiteUrl: 'https://modrinth.com/mod/abcdefgh' }, downloadCount: 12500, updatedAt: '2026-10-24T00:00:00Z' };
const version = { id: 'versiona', modId: project.id, displayName: 'Fixture 1.0', fileName: 'fixture.jar', fileLength: 2800000, fileDate: '2026-10-24T00:00:00Z', releaseType: 1, sha1: 'a'.repeat(40) };
let planReads = 0, submissions = 0, planErrorMode = null;
let fixtureAccounts = [], loginOperation = null, loginMode = 'code', loginStarts = 0, historyDeletes = 0;
const fixtureAccount = { id: '00112233445566778899aabbccddeeff', username: '测试玩家', selected: true };


const server = createServer(async (req, res) => {
	const url = new URL(req.url, 'http://fixture');
	const fixture = data => { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(data)); };
	if (req.headers.authorization && url.pathname === '/clients') { fixture(clients); return; }
	const decoded = decodeURIComponent(url.pathname);
	if (decoded === '/accounts/login') {
		if (req.method !== 'POST') { fixture({ enabled: true, unavailableReason: null }); return; }
		loginStarts++;
		loginOperation = { id: loginStarts, state: loginMode === 'failed' ? 'failed' : loginMode === 'success' ? 'completed' : 'running', stage: 'waiting_for_approval', cancelRequested: false, authorization: loginMode === 'code' ? { kind: 'device_code', verificationUri: 'https://www.microsoft.com/link', userCode: 'FIXT-CODE', expiresInSeconds: 600, pollIntervalSeconds: 5, expiresAt: Math.floor(Date.now() / 1000) + 600 } : null, account: loginMode === 'success' ? fixtureAccount : null, error: loginMode === 'failed' ? 'expired fixture 原始错误' : null, errorCode: loginMode === 'failed' ? 'authentication_failed' : null };
		if (loginMode === 'success') fixtureAccounts = [fixtureAccount];
		fixture(loginOperation); return;
	}
	if (decoded === '/accounts') { fixture(fixtureAccounts); return; }
	if (decoded.endsWith('/skin')) { fixture(null); return; }
	if (decoded.endsWith('/verify')) { fixture({ valid: true }); return; }
	if (decoded === '/account-operations/active') { fixture({ operation: loginOperation }); return; }
	if (/^\/account-operations\/\d+$/.test(decoded)) { if (req.method === 'DELETE') loginOperation = { ...loginOperation, state: 'cancelled', authorization: null }; fixture(loginOperation); return; }
	if (decoded === '/play-stats' && fixtureAccounts.length) {
		if (req.method === 'DELETE') historyDeletes++;
		fixture({ schemaVersion: 1, accounts: { [fixtureAccount.id]: { totalMs: 3600000, launches: 4, lastPlayedAt: null, lastInstanceId: null, last7dMs: 3600000, instances: {}, recentSessions: [] } } }); return;
	}
	if (req.headers.authorization && decoded.endsWith('/install')) { submissions++; res.writeHead(500); fixture({ error: 'unexpected installation' }); return; }
	if (decoded === '/resources/curseforge/configuration') { fixture({ configured: false, issue: 'fixture 原始来源原因' }); return; }
	if (decoded === '/resources/modrinth/projects' || decoded === '/resources/modrinth/shader-projects') { fixture({ ...catalog, items: [project], nextIndex: null }); return; }
	if (/^\/resources\/modrinth\/(?:projects|shader-projects)\/[^/]+$/.test(decoded)) { fixture({ ...catalog, project }); return; }
	if (/^\/resources\/modrinth\/shader-projects\/[^/]+\/versions$/.test(decoded) || /\/modrinth\/projects\/[^/]+\/files$/.test(decoded)) { fixture({ ...catalog, items: [version], nextIndex: null }); return; }
	if (decoded.endsWith('/plan')) {
		planReads++;
		if (planErrorMode) { res.statusCode = 409; fixture({ error: 'fixture 原始计划错误', ...(planErrorMode === 'legacy' ? {} : { code: planErrorMode }) }); return; }
		fixture({ planId: 10, expiresInMs: 600000, revision: 0, gameVersion: '1.21.1', totalBytes: version.fileLength, downloadBytes: version.fileLength, items: [{ project: { ...project, id: decoded.includes('YL57xq9U') ? 'YL57xq9U' : project.id }, file: version, direct: true }], files: [{ projectId: project.id, fileId: version.id, projectName: project.name, filename: version.fileName, size: version.fileLength, direct: true, reuse: false }], optionalProjects: ['optional'], warnings: ['fixture legacy wording'], warningDetails: [{ code: 'fabric_required_only', message: 'changed source wording' }, { code: 'future_warning', message: 'fixture 未识别警告原文' }] }); return;
	}
	if (decoded.endsWith('/shader-plans')) { fixture({ planId: 11, name: clients[0].name, revision: 0, projectId: project.id, projectName: project.name, versionId: version.id, versionName: version.displayName, fileName: 'fixture.zip', fileSize: version.fileLength }); return; }
	if (req.headers.authorization && /^\/clients\/[^/]+$/.test(decoded)) { fixture({ ...clients[0], versionType: 'release', javaMajor: 21, os: 'macos', architecture: 'aarch64', fileCount: 10 }); return; }
	if (req.headers.authorization && decoded.endsWith('/resources')) { fixture({ schemaVersion: 5, revision: 0, appliedRevision: 0, mods: [], resourcePacks: [], shaderPacks: [] }); return; }
	if (req.headers.authorization && decoded.endsWith('/resource-packs')) { fixture({ revision: 0, appliedRevision: 0, packs: [] }); return; }
	if (req.headers.authorization && decoded.endsWith('/shader-packs')) { fixture({ revision: 0, appliedRevision: 0, packs: [], environment: { status: 'missing_iris', iris: null, sodium: null, managedOnly: true, reason: 'fixture 原始光影原因' } }); return; }
	if (req.headers.authorization && decoded.endsWith('/launch-settings')) { fixture({ schemaVersion: 1, revision: 0, maxMemoryMib: 2048, initialMemoryMib: null, resolution: null }); return; }
	if (decoded === '/system/memory') { fixture({ totalMib: 16384 }); return; }
	if (req.headers.authorization && decoded.endsWith('/logs/latest')) { fixture({ exists: true, text: '[INFO] fixture 游戏日志保持原文', totalBytes: 48, truncated: false }); return; }
	if (req.headers.authorization && decoded.endsWith('/crash-reports')) { fixture({ reports: [], truncated: false, maxReportBytes: 2097152 }); return; }
	if (url.pathname === '/play-stats') { fixture({ schemaVersion: 1, accounts: {} }); return; }
	if (url.pathname === '/news') { fixture({ items: [{ title: '官方原文 fixture', url: 'https://www.minecraft.net/article/fixture', publishedAt: '2026-11-03T00:00:00Z', categories: ['Java'] }], stale: false, freshForMs: 300000, revision: 1, refreshError: null }); return; }
	if (url.pathname === '/client/version-catalog') { fixture({ versions: [{ id: '1.21.1', type: 'release', releaseTime: '2024-08-08T00:00:00Z' }], stale: false, freshForMs: 300000, revision: 1, refreshError: null }); return; }
	if (url.pathname.startsWith('/client/fabric/versions/')) { fixture({ versions: [{ version: '0.16.10', stable: true }], stale: false, freshForMs: 300000, revision: 1, refreshError: null }); return; }
	if (url.pathname === '/health') { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify({ status: 'ok', apiVersion: '1.0.0' })); return; }
	if (['/accounts', '/operations', '/client-launches', '/modpack-exports', '/resource-operations'].includes(url.pathname)) {
		res.setHeader('Content-Type', 'application/json'); res.end('[]'); return;
	}
	try {
		const name = url.pathname === '/' || !path.extname(url.pathname) ? 'index.html' : decodeURIComponent(url.pathname.slice(1));
		const file = path.resolve(build, name);
		if (!file.startsWith(build + path.sep)) throw Error('invalid fixture path');
		const body = await readFile(file);
		res.setHeader('Content-Type', ({ '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.svg': 'image/svg+xml', '.ico': 'image/x-icon' })[path.extname(file)] ?? 'application/octet-stream');
		res.end(body);
	} catch { res.writeHead(404); res.end(); }
});
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
async function auditLayout(page, label) {
	await page.evaluate(async () => {
		await document.fonts.ready;
		await Promise.all(document.getAnimations().filter(animation => animation.effect?.getTiming().iterations !== Infinity).map(animation => animation.finished.catch(() => {})));
	});
	const issues = await page.evaluate(() => {
		const problems = [];
		if (document.documentElement.scrollWidth > innerWidth) problems.push('document overflows horizontally');
		for (const element of document.querySelectorAll('button, a[href], input:not([type="hidden"]), select, textarea, [role="scrollbar"]')) {
			if (element.closest('[inert], [aria-hidden="true"]')) continue;
			const rect = element.getBoundingClientRect(), style = getComputedStyle(element);
			if (!rect.width || !rect.height || style.visibility === 'hidden' || style.display === 'none' || rect.bottom <= 0 || rect.top >= innerHeight) continue;
			const reference = [...(element.getAttribute('aria-labelledby') ?? '').split(/\s+/)].map(id => document.getElementById(id)?.textContent ?? '').join(' ').trim();
			const name = element.getAttribute('aria-label') || reference || [...(element.labels ?? [])].map(label => label.textContent).join(' ').trim() || element.textContent.trim() || element.getAttribute('title');
			if (!name) problems.push(`unnamed ${element.tagName}`);
			if (rect.left < -1 || rect.right > innerWidth + 1) problems.push(`offscreen control: ${name}`);
			if (element.tagName === 'BUTTON' && element.scrollWidth > element.clientWidth + 2) problems.push(`clipped button: ${name}`);
		}
		return problems;
	});
	if (issues.length) console.error(label, issues);
	return issues.map(issue => `${label}: ${issue}`);
}

try {
	browser = await chromium.launch({ channel: process.env.ANVIL_BROWSER_CHANNEL ?? 'msedge', headless: true });
	async function context(blockStorage = false, runtimeState = 'ready', savedLocale = null) {
		const context = await browser.newContext({ viewport: { width: 900, height: 680 }, locale: 'zh-CN' });
		await context.route('**/*', route => route.request().url().startsWith(origin + '/') ? route.continue() : route.abort());
		await context.addInitScript(({ origin, blockStorage, runtimeState, savedLocale }) => {
			window.isTauri = true;
			window.__TAURI_INTERNALS__ = { metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }, invoke: async (command, args) => {
				if (command === 'shell_get_runtime_connection') return { baseUrl: origin, token: 'fixture', apiVersion: '1.0.0' };
				if (command === 'shell_get_runtime_status') return { state: runtimeState, runtimeVersion: '26.0.0-fixture', apiVersion: '1.0.0', availableRuntimeVersion: null };
				if (command === 'shell_get_app_info') return { appVersion: '26.0.0-fixture', appId: 'anvil.desktop', shellVersion: '26.0.0-fixture', shellApiVersion: '1.1.0', targetPlatform: 'fixture-platform' };
				if (command === 'shell_get_update_status') return window.__fixtureUpdate ?? { app: { state: 'up_to_date', failure: null, currentVersion: 'fixture', availableVersion: null, downloadedBytes: 0, totalBytes: null }, shell: { state: 'up_to_date', failure: null, availableVersion: null, releaseUrl: null } };
				if (command === 'shell_check_for_updates') { window.__fixtureChecks = (window.__fixtureChecks ?? 0) + 1; return { outcome: 'started', shellOutcome: 'started' }; }
				if (command === 'shell_get_logging_status') return { directory: '/fixture/logs', fileLoggingAvailable: true };
				if (command === 'shell_open_external_url') { window.__fixtureUrls = [...(window.__fixtureUrls ?? []), args.url]; return true; }
				return true;
			} };
			if (savedLocale && !localStorage.getItem('anvil.locale')) localStorage.setItem('anvil.locale', savedLocale);
			if (blockStorage) Object.defineProperty(window, 'localStorage', { get() { throw new DOMException('fixture denied', 'SecurityError'); } });
		}, { origin, blockStorage, runtimeState, savedLocale });
		return context;
	}
	async function chooseLanguage(page, value) {
		await page.locator('#app-language').click();
		const label = value === 'system' ? /System default|跟随系统/ : value === 'en' ? 'English' : '简体中文';
		const option = page.getByRole('option', { name: label, exact: true });
		if (value !== 'system') assert.equal(await option.locator('span[lang]').getAttribute('lang'), value);
		await option.click();
	}
	const normal = await context();
	const page = await normal.newPage();
	page.setDefaultTimeout(10000);
	await page.goto(`${origin}/settings`);
	await chooseLanguage(page, 'en');
	await page.getByRole('heading', { name: 'General', exact: true }).waitFor();
	assert.equal(await page.locator('html').getAttribute('lang'), 'en');
	assert.equal(await page.evaluate(() => localStorage.getItem('anvil.locale')), 'en');
	assert.equal(await page.title(), 'Settings · Anvil');
	await page.locator('#app-language').click();
	assert.equal(await page.getByRole('option', { name: '简体中文', exact: true }).locator('span[lang]').getAttribute('lang'), 'zh-CN');
	await page.keyboard.press('Escape');
	assert.equal(await page.locator('#app-language').evaluate(element => element === document.activeElement), true);
	await page.getByRole('navigation', { name: 'Main navigation', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Back', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Tasks', exact: true }).click();
	await page.getByText('No tasks in progress', { exact: true }).waitFor();
	await page.keyboard.press('Escape');
	await page.screenshot({ path: path.join(output, 'general-en.png') });
	await page.reload();
	await page.getByRole('heading', { name: 'General', exact: true }).waitFor();
	assert.equal(await page.locator('#app-language').innerText(), 'English');
	await page.getByRole('searchbox', { name: 'Search settings categories' }).fill('语言');
	await chooseLanguage(page, 'zh-CN');
	assert.equal(await page.getByRole('searchbox').inputValue(), '语言');
	await page.getByRole('heading', { name: '通用', exact: true }).waitFor();
	await page.getByRole('searchbox').fill('');
	await chooseLanguage(page, 'system');
	assert.equal(await page.evaluate(() => localStorage.getItem('anvil.locale')), null);
	assert.equal(await page.locator('html').getAttribute('lang'), 'zh-CN');
	await page.goto(`${origin}/settings?section=appearance`);
	await page.getByRole('heading', { name: '外观', exact: true }).waitFor();
	await page.getByRole('button', { name: '通用', exact: true }).click();
	await chooseLanguage(page, 'en');
	await page.getByRole('heading', { name: 'General', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Appearance', exact: true }).waitFor();
	assert.equal(await page.locator('#app-language').innerText(), 'English');
	await page.getByRole('button', { name: 'Servers', exact: true }).click();
	await page.getByText('Coming soon', { exact: true }).waitFor();
	assert.equal(await page.title(), 'Servers · Anvil');
	await page.getByRole('button', { name: 'Back', exact: true }).click();
	await page.getByRole('heading', { name: 'Appearance', exact: true }).waitFor();
	await page.goto(`${origin}/missing-fixture-page`);
	await page.getByRole('heading', { name: 'Could not open this page', exact: true }).waitFor();
	await page.getByRole('link', { name: 'Back to home', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Home', exact: true }).click();
	await page.getByRole('heading', { name: /^All instances/ }).waitFor();
	await page.getByText('No game instances yet', { exact: true }).waitFor();
	await page.getByText('Your next adventure', { exact: false }).waitFor();
	await page.getByText('官方原文 fixture', { exact: true }).waitFor();
	await page.screenshot({ path: path.join(output, 'home-en.png') });
	await page.getByRole('button', { name: 'Instances', exact: true }).click();
	await page.getByText('Manage your Minecraft Java instances', { exact: true }).waitFor();
	await page.locator('[data-client-install-trigger]').click();
	await page.getByRole('dialog').getByText('1.21.1', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Next', exact: true }).click();
	await page.getByRole('button', { name: /^Fabric/ }).click();
	await page.getByText('0.16.10', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Next', exact: true }).click();
	await page.locator('#client-instance-name').fill('海岛生存-test');
	await page.getByRole('button', { name: 'Start installation', exact: true }).waitFor();
	await page.locator('[data-install-step="3"]').evaluate(async element => { await Promise.all(element.getAnimations().map(animation => animation.finished)); });
	await page.screenshot({ path: path.join(output, 'installer-en.png') });
	await page.getByRole('button', { name: 'Close installer', exact: true }).click();
	clients = [{ id: 'client-fixture', name: '海岛生存', versionId: '1.21.1', loader: { kind: 'fabric', version: '0.16.10' } }];
	await page.getByRole('button', { name: 'Refresh instances', exact: true }).click();
	await page.getByRole('heading', { name: '海岛生存', exact: true }).waitFor();
	await page.getByRole('searchbox', { name: 'Search instances', exact: true }).fill('missing');
	await page.getByText('No matching instances', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Clear search', exact: true }).click();
	await page.getByRole('heading', { name: '海岛生存', exact: true }).waitFor();
	await page.screenshot({ path: path.join(output, 'instances-en.png') });
	await page.getByRole('link', { name: 'Manage instance 海岛生存', exact: true }).click();
	await page.getByRole('complementary', { name: 'Instance sidebar', exact: true }).waitFor();
	await page.getByText('0 shader packs', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Export', exact: true }).click();
	assert.equal(await page.getByRole('checkbox', { name: 'Worlds', exact: true }).isChecked(), false);
	assert.equal(await page.getByRole('checkbox', { name: 'Mods', exact: true }).isChecked(), true);
	await page.screenshot({ path: path.join(output, 'export-options-en.png') });
	await page.getByRole('dialog').getByRole('button', { name: 'Close', exact: true }).click();
	const sidebar = page.getByRole('navigation', { name: 'Instance navigation', exact: true });
	await sidebar.getByRole('link', { name: 'Instance settings', exact: true }).click();
	await page.getByRole('heading', { name: 'Instance settings', exact: true }).waitFor();
	await page.getByText('Maximum memory', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Save', exact: true }).waitFor();
	await sidebar.getByRole('link', { name: 'Resource packs', exact: true }).click();
	await page.getByText('No resource packs yet', { exact: true }).waitFor();
	await page.getByLabel('Choose local resource pack ZIP', { exact: true }).setInputFiles({ name: 'empty.zip', mimeType: 'application/zip', buffer: Buffer.alloc(0) });
	await page.getByRole('alert').filter({ hasText: 'non-empty ZIP' }).waitFor();
	await sidebar.getByRole('link', { name: 'Shaders', exact: true }).click();
	await page.getByText('Iris is not installed', { exact: true }).waitFor();
	await page.getByText('No shader packs yet', { exact: true }).waitFor();
	await page.screenshot({ path: path.join(output, 'instance-shaders-en.png') });
	await sidebar.getByRole('link', { name: 'Game log', exact: true }).click();
	await page.getByText('[INFO] fixture 游戏日志保持原文', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Crash reports', exact: true }).click();
	await page.getByText('No crash reports', { exact: true }).waitFor();

	await page.goto(`${origin}/mods?instance=${encodeURIComponent('海岛生存')}`);
	await page.getByRole('searchbox', { name: 'Search mods', exact: true }).waitFor();
	await page.getByText('Fixture 模组原名', { exact: true }).click();
	await page.getByRole('region', { name: 'Mod details', exact: true }).waitFor();
	await page.getByRole('button', { name: /^Fixture 1.0/ }).click();
	await page.getByRole('button', { name: 'Confirm installation', exact: true }).waitFor();
	await page.getByRole('alert').filter({ hasText: 'unrecognized notice' }).waitFor();
	await page.getByRole('button', { name: 'Expand installation details', exact: true }).click();
	await page.getByText('1 file · 0 reused', { exact: true }).waitFor();
	await page.getByText('1 optional dependency is not included.', { exact: true }).waitFor();
	await page.getByText(/Only compatible Fabric JARs/).waitFor();
	assert.equal(planReads, 1);
	assert.equal(submissions, 0);
	await page.screenshot({ path: path.join(output, 'mod-plan-en.png') });
	// Expanding details never re-reads the plan or submits an install.
	await page.getByRole('button', { name: 'Collapse installation details', exact: true }).click();
	assert.equal(planReads, 1);
	planErrorMode = 'plan_expired';
	await page.getByRole('button', { name: /^Fixture 1.0/ }).click();
	await page.getByRole('alert').filter({ hasText: 'The installation plan expired. Generate a new plan.' }).waitFor();
	await page.screenshot({ path: path.join(output, 'runtime-error-code-en.png') });
	for (const [mode, text] of [['future_error', 'The operation failed. Please try again.'], ['legacy', 'Could not prepare an installation plan.']]) {
		planErrorMode = mode;
		await page.getByRole('button', { name: 'Generate again', exact: true }).click();
		await page.getByRole('alert').filter({ hasText: text }).waitFor();
		await page.getByText('Technical details', { exact: true }).click();
		await page.getByText('fixture 原始计划错误', { exact: true }).waitFor();
	}
	planErrorMode = null;
	assert.equal(submissions, 0);

	await page.goto(`${origin}/mods?source=curseforge`);
	await page.getByRole('alert').filter({ hasText: 'CurseForge is not configured' }).waitFor();
	await page.goto(`${origin}/shaders?instance=${encodeURIComponent('海岛生存')}`);
	await page.getByRole('searchbox', { name: 'Search shaders', exact: true }).waitFor();
	await page.getByText('Fixture 模组原名', { exact: true }).click();
	await page.getByRole('button', { name: /^Fixture 1.0/ }).click();
	await page.getByRole('button', { name: 'Download and import', exact: true }).waitFor();
	assert.equal(submissions, 0);
	await page.screenshot({ path: path.join(output, 'shader-plan-en.png') });
	await page.goto(`${origin}/clients/${encodeURIComponent('海岛生存')}/shader-packs`);
	await page.getByRole('button', { name: 'Install shader support', exact: true }).click();
	await page.getByRole('button', { name: /^Fixture 1.0/ }).click();
	await page.getByRole('button', { name: 'Confirm installation', exact: true }).waitFor();
	assert.equal(submissions, 0);
	await page.screenshot({ path: path.join(output, 'iris-plan-en.png') });
	await page.goto(`${origin}/settings?section=appearance`);
	await page.getByRole('heading', { name: 'Visual style', exact: true }).waitFor();
	assert.equal(await page.locator('#theme-preset option[value="default"]').innerText(), 'Shulker Purple');
	await page.locator('#theme-preset').selectOption('custom');
	await page.getByRole('textbox', { name: 'Page background hexadecimal color', exact: true }).fill('bad');
	await page.getByRole('textbox', { name: 'Page background hexadecimal color', exact: true }).press('Tab');
	await page.getByRole('alert').filter({ hasText: 'Enter a color in #RRGGBB format' }).waitFor();
	await page.screenshot({ path: path.join(output, 'appearance-en.png') });
	await page.locator('#theme-preset').selectOption('default');
	await page.getByRole('button', { name: 'Account', exact: true }).click();
	await page.getByRole('menu', { name: 'Account menu', exact: true }).waitFor();
	await page.getByRole('menuitem', { name: 'Sign in with Microsoft', exact: true }).click();
	await page.getByRole('dialog', { name: 'Sign in with Microsoft', exact: true }).waitFor();
	await page.getByRole('button', { name: 'Continue', exact: true }).click();
	await page.getByText('FIXT-CODE', { exact: true }).waitFor();
	await page.getByText('Waiting for browser authorization…', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Open verification page', exact: true }).waitFor();
	assert.equal(loginStarts, 1);
	assert.deepEqual(await page.evaluate(() => window.__fixtureUrls), ['https://www.microsoft.com/link']);
	await page.screenshot({ path: path.join(output, 'login-code-en.png') });
	await page.getByRole('button', { name: 'Cancel sign-in', exact: true }).click();
	await page.getByText('Sign-in cancelled.', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Close sign-in window', exact: true }).click();
	loginMode = 'failed';
	await page.getByRole('button', { name: 'Account', exact: true }).click();
	await page.getByRole('menuitem', { name: 'Sign in with Microsoft', exact: true }).click();
	await page.getByRole('button', { name: 'Continue', exact: true }).click();
	await page.getByRole('alert').filter({ hasText: 'Account verification failed' }).waitFor();
	await page.getByText('Error details', { exact: true }).click();
	await page.getByText('expired fixture 原始错误', { exact: true }).waitFor();
	await page.screenshot({ path: path.join(output, 'login-failed-en.png') });
	await page.getByRole('button', { name: 'Close sign-in window', exact: true }).click();
	fixtureAccounts = [fixtureAccount];
	await page.goto(`${origin}/settings?section=accounts`);
	await page.getByText('测试玩家 · 4 launches · 1.0 hour', { exact: true }).waitFor();
	await page.getByRole('button', { name: 'Clear history', exact: true }).click();
	await page.getByRole('dialog', { name: 'Clear play history for 测试玩家?', exact: true }).waitFor();
	await page.screenshot({ path: path.join(output, 'history-confirm-en.png') });
	await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
	await page.getByRole('button', { name: 'Clear all', exact: true }).click();
	await page.getByRole('dialog', { name: 'Clear all play history on this device?', exact: true }).waitFor();
	await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
	assert.equal(historyDeletes, 0);
	await page.getByRole('button', { name: 'Account: 测试玩家', exact: true }).click();
	await page.getByRole('menu').getByText('Minecraft Java Edition', { exact: true }).waitFor();
	await page.getByRole('menuitem', { name: 'Switch account', exact: true }).hover();
	await page.getByRole('menuitem', { name: /测试玩家/ }).waitFor();
	await page.getByRole('img', { name: 'Current account', exact: true }).waitFor();
	await page.getByRole('menuitem', { name: 'Sign out', exact: true }).waitFor();
	await page.screenshot({ path: path.join(output, 'account-menu-en.png') });
	await page.keyboard.press('Escape');
	await page.goto(`${origin}/settings?section=about`);
	await page.getByRole('heading', { name: 'Version information', exact: true }).waitFor();
	await page.getByText('/fixture/logs', { exact: true }).waitFor();
	await page.evaluate(() => { window.__fixtureUpdate = { app: { state: 'failed', failure: 'shell_upgrade_required', availableVersion: '26.1.0', currentVersion: 'fixture', downloadedBytes: 0, totalBytes: null }, shell: { state: 'up_to_date', availableVersion: null, releaseUrl: null } }; });
	await page.getByRole('button', { name: 'Check for updates', exact: true }).click();
	await page.getByText(/Desktop 26.1.0 requires a newer launcher/).waitFor();
	await page.getByRole('button', { name: 'View installers', exact: true }).waitFor();
	assert.equal(await page.evaluate(() => window.__fixtureChecks), 1);
	await page.screenshot({ path: path.join(output, 'about-upgrade-en.png') });
	await normal.close();
	const blocked = await context(true);
	const blockedPage = await blocked.newPage();
	await blockedPage.goto(`${origin}/settings`);
	await blockedPage.getByRole('status').filter({ hasText: '无法读取已保存的语言' }).waitFor();
	await chooseLanguage(blockedPage, 'en');
	await blockedPage.getByRole('status').filter({ hasText: 'only to this window' }).waitFor();
	assert.equal(await blockedPage.locator('html').getAttribute('lang'), 'en');
	await blockedPage.screenshot({ path: path.join(output, 'general-storage-blocked.png') });
	await blocked.close();
	const failed = await context(false, 'failed', 'en');
	const failedPage = await failed.newPage();
	await failedPage.goto(`${origin}/settings`);
	await failedPage.getByRole('alert').filter({ hasText: 'Could not connect to Runtime' }).waitFor();
	await failedPage.getByRole('button', { name: 'Retry', exact: true }).waitFor();
	await failedPage.getByRole('button', { name: 'View logs', exact: true }).waitFor();
	await failedPage.screenshot({ path: path.join(output, 'startup-error-en.png') });
	await failed.close();

	const layoutIssues = [];
	const longName = 'Long-instance-name-for-layout-验收';
	clients = [{ id: 'client-fixture', name: longName, versionId: '1.21.1', loader: { kind: 'fabric', version: '0.16.10' } }];
	for (const locale of ['zh-CN', 'en']) {
		const copy = locale === 'en'
			? { home: /^All instances/, clients: /^Instances/, general: 'General', appearance: 'Appearance', about: 'Version information', accounts: 'Accounts', overview: longName, mods: 'Mods', localResources: 'Resource packs', localShaders: 'Shaders', instanceSettings: 'Instance settings', logs: 'Game log', onlineMods: project.name, onlineShaders: 'Shaders', confirm: 'Confirm installation', download: 'Download and import', next: 'Next', fabric: /^Fabric/, close: 'Close installer', start: 'Start installation', cards: ['Mods', 'Resource packs', 'Shaders'] }
			: { home: /^全部实例/, clients: /^客户端/, general: '通用', appearance: '外观', about: '版本信息', accounts: '账户', overview: longName, mods: '模组', localResources: '资源包', localShaders: '光影', instanceSettings: '实例设置', logs: '游戏日志', onlineMods: project.name, onlineShaders: '光影', confirm: '确认安装', download: '确认下载并导入', next: '下一步', fabric: /^Fabric/, close: '关闭安装窗口', start: '开始安装', cards: ['模组', '资源包', '光影'] };
		for (const width of [620, 1280]) {
			const matrix = await context(false, 'ready', locale);
			const check = await matrix.newPage();
			check.setDefaultTimeout(10000);
			await check.setViewportSize({ width, height: width === 620 ? 420 : 800 });
			for (const [name, route] of [
				['home', '/'], ['clients', '/clients'], ['general', '/settings'], ['appearance', '/settings?section=appearance'],
				['accounts', '/settings?section=accounts'], ['about', '/settings?section=about'],
				['overview', `/clients/${encodeURIComponent(longName)}`], ['mods', `/clients/${encodeURIComponent(longName)}/mods`],
				['localResources', `/clients/${encodeURIComponent(longName)}/resource-packs`], ['localShaders', `/clients/${encodeURIComponent(longName)}/shader-packs`],
				['instanceSettings', `/clients/${encodeURIComponent(longName)}/settings`], ['logs', `/clients/${encodeURIComponent(longName)}/logs`],
				['onlineMods', `/mods?instance=${encodeURIComponent(longName)}&project=abcdefgh`], ['onlineShaders', `/shaders?instance=${encodeURIComponent(longName)}`],
			]) {
				await check.goto(origin + route);
				await check.getByRole('heading', { name: copy[name], exact: typeof copy[name] === 'string' }).first().waitFor();
				if (name === 'onlineShaders') await check.getByText(project.name, { exact: true }).click();
				if (name === 'onlineMods' || name === 'onlineShaders') {
					await check.getByRole('button', { name: /^Fixture 1.0/ }).click();
					const action = check.getByRole('button', { name: name === 'onlineMods' ? copy.confirm : copy.download, exact: true });
					await action.waitFor();
					const rect = await action.boundingBox();
					if (!rect || rect.y < 32 || rect.y + rect.height > (width === 620 ? 420 : 800)) layoutIssues.push(`${locale}-${width}-${name}: confirmation action outside viewport`);
				}
				await check.waitForFunction(() => !document.querySelector('[aria-busy="true"]'));
				if (name === 'appearance' && width === 620) {
					await check.locator('section[aria-labelledby="settings-title"]').evaluate(element => { element.scrollTop = 50; element.dispatchEvent(new Event('scroll', { bubbles: true })); });
					await check.getByRole('scrollbar', { name: locale === 'en' ? 'Vertical scrollbar' : '垂直滚动条', exact: true }).first().waitFor();
				}
				layoutIssues.push(...await auditLayout(check, `${locale}-${width}-${name}`));
				if (name === 'overview') {
					for (const title of copy.cards) {
						const heading = check.getByRole('heading', { name: title, exact: true });
						if (await heading.evaluate(element => element.scrollWidth > element.clientWidth + 1)) layoutIssues.push(`${locale}-${width}-overview: clipped resource card ${title}`);
					}
				}
				if (name === 'clients') {
					await check.locator('[data-client-install-trigger]').click();
					await check.getByRole('dialog').getByText('1.21.1', { exact: true }).waitFor();
					await check.getByRole('button', { name: copy.next, exact: true }).click();
					await check.getByRole('button', { name: copy.fabric }).click();
					await check.getByText('0.16.10', { exact: true }).waitFor();
					await check.getByRole('button', { name: copy.next, exact: true }).click();
					await check.getByRole('button', { name: copy.start, exact: true }).waitFor();
					layoutIssues.push(...await auditLayout(check, `${locale}-${width}-installer`));
					await check.screenshot({ path: path.join(output, `layout-${locale}-${width}-installer.png`) });
					await check.keyboard.press('Escape');
					assert.equal(await check.locator('[data-client-install-trigger]').evaluate(element => element === document.activeElement), true);
				}
				await check.screenshot({ path: path.join(output, `layout-${locale}-${width}-${name}.png`) });
			}
			await matrix.close();
		}
	}
	assert.deepEqual(layoutIssues, []);
	console.log('PASS: language persistence/search/storage, translated navigation/tasks, home/install, instance settings/packs/shaders/logs/export, online mods/shaders/Iris plans, settings/account/login/update, error pages, and bilingual 620/1280 layout/name checks');
} catch (error) {
	const failedPage = browser?.contexts().at(-1)?.pages().at(-1);
	if (failedPage) {
		console.error(await failedPage.locator('body').innerText());
		await failedPage.screenshot({ path: path.join(output, 'failure.png') });
	}
	throw error;
} finally {
	await browser?.close();
	await new Promise(resolve => server.close(resolve));
}
