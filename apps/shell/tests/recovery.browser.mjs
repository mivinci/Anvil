// Opt-in: real browser/CSP, embedded files only, no Tauri process or installed App.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.ANVIL_PLAYWRIGHT_MODULE ?? 'playwright');
const bootstrap = fileURLToPath(new URL('../bootstrap/', import.meta.url));
const output = fileURLToPath(new URL('../../../target/shell-recovery-qa/', import.meta.url));
await mkdir(output, { recursive: true });
const pageHtml = await readFile(path.join(bootstrap, 'index.html'));
const script = await readFile(path.join(bootstrap, '__anvil_shell__/recovery.js'));
let pageReads = 0;
const server = createServer((req, res) => {
	const url = new URL(req.url, 'http://fixture');
	res.setHeader('Cache-Control', 'no-store');
	res.setHeader('X-Content-Type-Options', 'nosniff');
	if (url.pathname === '/') { pageReads++; res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end(pageHtml); }
	else if (url.pathname === '/__anvil_shell__/recovery.js') { res.setHeader('Content-Type', 'text/javascript; charset=utf-8'); res.end(script); }
	else { res.writeHead(404); res.end(); }
});
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
	browser = await chromium.launch({ channel: process.env.ANVIL_BROWSER_CHANNEL ?? 'msedge', headless: true });
	for (const [name, languages, language, expected] of [
		['english', ['fr-FR', 'en-US'], 'fr-FR', 'en'],
		['chinese', ['zh-Hans', 'en'], 'en', 'zh-CN'],
		['fallback', ['de-DE'], 'en-US', 'zh-CN'],
		['language-only', [], 'en-GB', 'en'],
	]) {
		const context = await browser.newContext({ viewport: { width: 900, height: 620 }, locale: language });
		await context.route('**/*', route => route.request().url().startsWith(origin + '/') ? route.continue() : route.abort());
		await context.addInitScript(({ languages, language }) => {
			Object.defineProperty(navigator, 'languages', { get: () => languages });
			Object.defineProperty(navigator, 'language', { get: () => language });
			Object.defineProperty(window, 'localStorage', { get() { throw Error('App storage is unavailable in recovery'); } });
			window.__fixtureViolations = [];
			document.addEventListener('securitypolicyviolation', event => window.__fixtureViolations.push(event.violatedDirective));
		}, { languages, language });
		const page = await context.newPage();
		await page.goto(origin);
		await page.getByRole('heading', { name: expected === 'en' ? 'Anvil could not start the desktop App' : 'Anvil 暂时无法启动桌面 App', exact: true }).waitFor();
		assert.equal(await page.locator('html').getAttribute('lang'), expected);
		assert.equal(await page.title(), expected === 'en' ? 'Anvil — Recovery' : 'Anvil — 恢复');
		assert.equal(await page.locator('title').count(), 1);
		const retry = page.getByRole('button', { name: expected === 'en' ? 'Retry' : '重试', exact: true });
		assert.equal(await retry.count(), 1);
		const reads = pageReads;
		await Promise.all([page.waitForEvent('load'), retry.click()]);
		assert.equal(pageReads, reads + 1);
		assert.deepEqual(await page.evaluate(() => window.__fixtureViolations), []);
		assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
		await page.screenshot({ path: path.join(output, `${name}.png`) });
		await context.close();
	}
	const staticContext = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 620, height: 420 } });
	const staticPage = await staticContext.newPage();
	await staticPage.goto(origin);
	await staticPage.getByText('Anvil could not start the desktop App', { exact: true }).waitFor();
	await staticPage.getByText('Anvil 暂时无法启动桌面 App', { exact: true }).waitFor();
	assert.equal(await staticPage.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
	await staticPage.screenshot({ path: path.join(output, 'static-bilingual.png'), fullPage: true });
	await staticContext.close();
	console.log(`PASS: recovery system language/fallback, CSP-compatible retry and static bilingual content; screenshots: ${output}`);
} finally {
	await browser?.close();
	await new Promise(resolve => server.close(resolve));
}
