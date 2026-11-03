// Opt-in offline avatar acceptance. Build first; provide Playwright via ANVIL_PLAYWRIGHT_MODULE.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.ANVIL_PLAYWRIGHT_MODULE ?? 'playwright');
const build = path.resolve(fileURLToPath(new URL('../build/', import.meta.url)));
const output = path.resolve(build, '../.svelte-kit/instance-avatar-qa');
await mkdir(output, { recursive: true });

const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=', 'base64');
const TOKEN = 'fixture-token';
const ORIGIN = 'http://127.0.0.1:0';
let avatar = null;
let putFailure = null;
let avatarFailure = false;
let puts = 0;
let deletes = 0;
const requests = [];
const client = { id: 'client-fixture', name: 'Fixture', versionId: '1.21.1', loader: { kind: 'fabric', version: '0.16.10' } };

function json(res, value, status = 200) {
	res.writeHead(status, { 'Content-Type': 'application/json' });
	res.end(JSON.stringify(value));
}


const server = createServer(async (req, res) => {
	const url = new URL(req.url, 'http://fixture');
	const pathname = decodeURIComponent(url.pathname);
	const lower = pathname.toLowerCase();
	requests.push(`${req.method} ${pathname}`);
	if (req.headers['sec-fetch-mode'] === 'navigate' || (req.headers.accept ?? '').includes('text/html')) {
		res.writeHead(200, { 'Content-Type': 'text/html' });
		res.end(await readFile(path.join(build, 'index.html')));
		return;
	}
	if (req.headers.authorization !== `Bearer ${TOKEN}` && pathname.startsWith('/clients/')) { json(res, { error: 'unauthorized', code: 'unauthorized' }, 401); return; }
	if (pathname === '/health') { json(res, { status: 'ok', apiVersion: '1.0.0' }); return; }
	if (pathname === '/clients' && req.method === 'GET') { json(res, [client]); return; }
	if (lower === '/clients/fixture/avatar') {
		if (req.method === 'GET') {
			if (avatarFailure) { json(res, { error: 'fixture read failure', code: 'internal' }, 503); return; }
			if (!avatar) { json(res, { error: '没有头像', code: 'avatar_not_found' }, 404); return; }
			res.writeHead(200, { 'Content-Type': 'image/png', ETag: `"${avatar.revision}"`, 'Cache-Control': 'private, max-age=0, must-revalidate' });
			res.end(avatar.bytes); return;
		}
		if (req.method === 'PUT') {
			const chunks = [];
			for await (const chunk of req) chunks.push(chunk);
			puts++;
			if (putFailure === 'busy') { json(res, { error: '实例正在运行', code: 'busy' }, 409); return; }
			if (putFailure === 'invalid') { json(res, { error: 'invalid image', code: 'invalid' }, 400); return; }
			if (putFailure === 'missing') { json(res, { error: 'not found', code: 'not_found' }, 404); return; }
			const bytes = Buffer.concat(chunks);
			avatar = { bytes, revision: createHash('sha256').update(bytes).digest('hex') };
			json(res, { revision: avatar.revision }); return;
		}
		if (req.method === 'DELETE') { deletes++; avatar = null; res.writeHead(204); res.end(); return; }
	}
	if (lower === '/clients/fixture' && req.method === 'GET') { json(res, { ...client, versionType: 'release', javaMajor: 21, os: 'macos', architecture: 'aarch64', fileCount: 10 }); return; }
	if (lower === '/clients/fixture/resources') { json(res, { schemaVersion: 5, revision: 0, appliedRevision: 0, mods: [], resourcePacks: [], shaderPacks: [] }); return; }
	if (lower === '/clients/fixture/resource-packs') { json(res, { revision: 0, appliedRevision: 0, packs: [] }); return; }
	if (lower === '/clients/fixture/shader-packs') { json(res, { revision: 0, appliedRevision: 0, packs: [], environment: { status: 'missing_iris', iris: null, sodium: null, managedOnly: true, reason: 'fixture' } }); return; }
	if (lower === '/clients/fixture/launch-settings') { json(res, { schemaVersion: 1, revision: 0, maxMemoryMib: 2048, initialMemoryMib: null, resolution: null }); return; }
	if (lower === '/clients/fixture/logs/latest') { json(res, { exists: false, text: '', totalBytes: 0, truncated: false }); return; }
	if (lower === '/clients/fixture/crash-reports') { json(res, { reports: [], truncated: false, maxReportBytes: 2097152 }); return; }
	if (pathname === '/accounts') { json(res, []); return; }
	if (pathname === '/account-operations/active') { json(res, { operation: null }); return; }
	if (pathname === '/play-stats') { json(res, { schemaVersion: 1, accounts: {} }); return; }
	if (pathname === '/client-launches') { json(res, []); return; }
	if (pathname === '/news') { json(res, { items: [], stale: false, freshForMs: 300000, revision: 1, refreshError: null }); return; }
	if (pathname === '/client/version-catalog') { json(res, { versions: [{ id: '1.21.1', type: 'release', releaseTime: '2024-08-08T00:00:00Z' }], stale: false, freshForMs: 300000, revision: 1, refreshError: null }); return; }
	if (pathname === '/system/memory') { json(res, { totalMib: 16384 }); return; }
	if (['/operations', '/modpack-exports', '/resource-operations'].includes(pathname)) { json(res, []); return; }
	try {
		const requested = path.resolve(build, `.${pathname}`);
		if (requested !== path.resolve(build) && !requested.startsWith(build + path.sep)) throw new Error('invalid fixture path');
		let body, extension;
		try { body = await readFile(requested); extension = path.extname(requested); }
		catch { body = await readFile(path.join(build, 'index.html')); extension = '.html'; }
		res.writeHead(200, { 'Content-Type': ({ '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.svg': 'image/svg+xml' })[extension] ?? 'application/octet-stream' });
		res.end(body);
	} catch { res.writeHead(404); res.end(); }
});
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
const origin = `http://127.0.0.1:${server.address().port}`;

let browser;
let page;
let errors = [];
try {
	browser = await chromium.launch({ channel: process.env.ANVIL_BROWSER_CHANNEL ?? 'msedge', headless: true });
	const context = await browser.newContext({ viewport: { width: 900, height: 680 }, locale: 'zh-CN' });
	await context.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
	await context.addInitScript(({ origin }) => {
		localStorage.setItem('anvil.welcome', 'complete');
		localStorage.setItem('anvil.locale', 'zh-CN');
		window.isTauri = true;
		window.__TAURI_INTERNALS__ = { metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }, invoke: async (command) => {
			if (command === 'shell_get_runtime_connection') return { baseUrl: origin, token: 'fixture-token', apiVersion: '1.0.0' };
			if (command === 'shell_get_runtime_status') return { state: 'ready', runtimeVersion: '26.0.0-fixture', apiVersion: '1.0.0', availableRuntimeVersion: null };
			if (command === 'shell_get_app_info') return { appVersion: '26.0.0-fixture', appId: 'anvil.desktop', shellVersion: '26.0.0-fixture', shellApiVersion: '1.1.0', targetPlatform: 'fixture-platform' };
			if (command === 'shell_get_update_status') return { app: { state: 'up_to_date', failure: null, currentVersion: 'fixture', availableVersion: null, downloadedBytes: 0, totalBytes: null }, shell: { state: 'up_to_date', failure: null, availableVersion: null, releaseUrl: null } };
			if (command === 'shell_get_logging_status') return { directory: '/fixture/logs', fileLoggingAvailable: true };
			return true;
		} };
	}, { origin });
	page = await context.newPage();
	page.setDefaultTimeout(15000);
	errors = [];
	page.on('pageerror', error => errors.push(error.message));
	await page.goto(`${origin}/clients`);
	const card = page.getByRole('link', { name: /Fixture/ }).first();
	await card.waitFor();
	await page.screenshot({ path: path.join(output, 'clients-card.png') });
	await card.click();
	await page.getByRole('heading', { name: 'Fixture', exact: true }).waitFor();
	assert.equal(avatar, null, 'initial fixture must start without a custom avatar');
	assert.equal(await page.locator('img[src^="blob:"]').count(), 0);
	const input = page.locator('input[type="file"][aria-label="更换实例头像"]');
	await input.setInputFiles({ name: 'huge.png', mimeType: 'image/png', buffer: Buffer.alloc(8 * 1024 * 1024 + 1) });
	await page.getByRole('heading', { name: '裁剪头像', exact: true }).waitFor();
	await page.getByText('图片不能为空，且不能超过 8 MiB。', { exact: true }).waitFor();
	assert.equal(puts, 0, 'over-limit files must not be uploaded');
	await page.getByRole('button', { name: '取消', exact: true }).click();
	await input.setInputFiles({ name: 'fixture.png', mimeType: 'image/png', buffer: PNG });
	await page.getByRole('heading', { name: '裁剪头像', exact: true }).waitFor();
	const preview = page.getByRole('button', { name: '头像裁剪预览' });
	await preview.focus();
	await preview.press('ArrowRight');
	await preview.press('+');
	await page.getByRole('button', { name: '应用', exact: true }).click();
	await page.getByText('头像已更新。', { exact: true }).waitFor();
	assert.equal(puts, 1);
	assert.equal(await page.locator('img[src^="blob:"]').count(), 1);
	await page.screenshot({ path: path.join(output, 'detail-avatar.png') });

	await page.reload();
	await page.getByRole('heading', { name: 'Fixture', exact: true }).waitFor();
	await page.locator('img[src^="blob:"]').first().waitFor();
	assert.equal(requests.filter(value => value === 'GET /clients/Fixture/avatar').length >= 1, true);

	putFailure = 'busy';
	await page.locator('input[type="file"][aria-label="更换实例头像"]').setInputFiles({ name: 'fail.png', mimeType: 'image/png', buffer: PNG });
	await page.getByRole('heading', { name: '裁剪头像', exact: true }).waitFor();
	await page.getByRole('button', { name: '应用', exact: true }).click();
	await page.getByText('实例正在运行或任务进行中，暂时不能修改头像。', { exact: true }).waitFor();
	assert.equal(puts, 2);
	assert.equal(deletes, 0);
	assert.equal(await page.locator('img[src^="blob:"]').count(), 1, 'failed upload must preserve the previous avatar');

	putFailure = 'invalid';
	await page.locator('input[type="file"][aria-label="更换实例头像"]').setInputFiles({ name: 'invalid.png', mimeType: 'image/png', buffer: PNG });
	await page.getByRole('heading', { name: '裁剪头像', exact: true }).waitFor();
	await page.getByRole('button', { name: '应用', exact: true }).click();
	await page.getByText('无法使用这张图片，请换一张。', { exact: true }).waitFor();
	assert.equal(puts, 3);
	assert.equal(await page.locator('img[src^="blob:"]').count(), 1, 'invalid upload must preserve the previous avatar');

	putFailure = 'missing';
	await page.locator('input[type="file"][aria-label="更换实例头像"]').setInputFiles({ name: 'missing.png', mimeType: 'image/png', buffer: PNG });
	await page.getByRole('heading', { name: '裁剪头像', exact: true }).waitFor();
	await page.getByRole('button', { name: '应用', exact: true }).click();
	await page.getByText('当前 Runtime 尚未支持头像接口，请完全退出并重启 Anvil。', { exact: true }).waitFor();
	assert.equal(puts, 4);
	assert.equal(await page.locator('img[src^="blob:"]').count(), 1, 'outdated Runtime must preserve the previous avatar');

	putFailure = null;
	await page.setViewportSize({ width: 620, height: 420 });
	await page.locator('input[type="file"][aria-label="更换实例头像"]').setInputFiles({ name: 'narrow.png', mimeType: 'image/png', buffer: PNG });
	await page.getByRole('heading', { name: '裁剪头像', exact: true }).waitFor();
	assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
	const box = await page.getByRole('dialog').boundingBox();
	assert.ok(box && box.x >= 0 && box.y >= 0 && box.x + box.width <= 620 && box.y + box.height <= 420);
	await page.keyboard.press('Escape');
	await page.getByRole('button', { name: '取消', exact: true }).click().catch(() => {});
	await page.goto(`${origin}/clients/fixture/settings`);
	await page.getByRole('button', { name: '恢复默认头像', exact: true }).waitFor();
	await page.getByRole('button', { name: '恢复默认头像', exact: true }).click();
	await page.getByText('已恢复默认头像。', { exact: true }).waitFor();
	assert.equal(deletes, 1);
	assert.equal(await page.locator('img[src^="blob:"]').count(), 0);
	avatarFailure = true;
	await page.reload();
	await page.getByRole('heading', { name: '实例设置', exact: true }).waitFor();
	assert.equal(await page.locator('img[src^="blob:"]').count(), 0);
	assert.equal(errors.length, 0, errors.join('\n'));
	avatarFailure = false;
	await page.screenshot({ path: path.join(output, 'settings-avatar-restored.png') });
	await context.close();
	console.log('instance avatar browser acceptance passed');
} catch (error) {
	console.error(error);
	if (typeof page !== 'undefined') { await page.screenshot({ path: path.join(output, 'failure.png'), fullPage: true }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(0, 3000)); console.error('page errors:', errors); console.error('requests:', requests.slice(-20)); }
	throw error;
} finally {
	await browser?.close();
	server.close();
}
