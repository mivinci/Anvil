import assert from 'node:assert/strict';
import { test, after } from 'node:test';
import { readFile, writeFile, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import ts from 'typescript';
const root = fileURLToPath(new URL('../../../', import.meta.url));
await mkdir(resolve(root, 'target'), { recursive: true });
const directory = await mkdtemp(resolve(root, 'target/runtime-request-'));
after(() => rm(directory, { recursive: true, force: true }));
const source = await readFile(resolve(root, 'apps/desktop/src/lib/runtime.ts'), 'utf8');
const javascript = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
	.replace(/import \{ shell \} from ['"]\.\/shell['"];?/, `const shell = { getRuntimeConnection: async () => ({ baseUrl: 'http://127.0.0.1:1', token: 'fixture', apiVersion: '1.0.0' }) };`);
const file = resolve(directory, 'runtime.mjs');
await writeFile(file, javascript);
const { listAccounts, RuntimeRequestError } = await import(pathToFileURL(file).href);

test('HTTP transport exposes additive code and original message while accepting old and malformed responses', async () => {
	const original = globalThis.fetch;
	let calls = 0;
	try {
		for (const [payload, expected] of [
			[{ error: 'fixture 原始详情', code: 'account_expired' }, 'account_expired'],
			[{ error: 'legacy fixture 原始详情' }, null],
			[{ error: 'future fixture 原始详情', code: 'future_error' }, 'future_error'],
			[{ error: 'invalid field', code: { anything: true } }, null],
		]) {
			globalThis.fetch = async () => { calls++; return new Response(JSON.stringify(payload), { status: 409 }); };
			await assert.rejects(listAccounts(), error => {
				assert.ok(error instanceof RuntimeRequestError);
				assert.equal(error.status, 409);
				assert.equal(error.code, expected);
				assert.equal(error.message, payload.error);
				return true;
			});
		}
		assert.equal(calls, 4);
	} finally { globalThis.fetch = original; }
});
