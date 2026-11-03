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
const { listAccounts, RuntimeRequestError, getClientAvatar, saveClientAvatar, clearClientAvatar } = await import(pathToFileURL(file).href);

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

test('avatar transport keeps auth headers, returns blob identity and accepts 204 clear', async () => {
	const original = globalThis.fetch;
	const calls = [];
	try {
		globalThis.fetch = async (url, init) => {
			calls.push({ url: String(url), init });
			if (init?.method === 'PUT') return new Response(JSON.stringify({ revision: 'r'.repeat(64) }), { status: 200, headers: { 'Content-Type': 'application/json' } });
			if (init?.method === 'DELETE') return new Response(null, { status: 204 });
			return new Response(new Blob([new Uint8Array([137, 80, 78, 71])], { type: 'image/png' }), { status: 200, headers: { ETag: '"revision"', 'Content-Type': 'image/png' } });
		};
		const signal = new AbortController().signal;
		const image = await getClientAvatar('client-one', signal);
		assert.equal(image?.revision, 'revision');
		assert.deepEqual([...new Uint8Array(await image.blob.arrayBuffer())], [137, 80, 78, 71]);
		const change = await saveClientAvatar('client-one', new Blob([new Uint8Array([1, 2])], { type: 'image/png' }), signal);
		assert.equal(change.revision, 'r'.repeat(64));
		await clearClientAvatar('client-one', signal);
		assert.equal(calls.length, 3);
		assert.equal(calls[1].init.method, 'PUT');
		assert.equal(new Headers(calls[1].init.headers).get('Content-Type'), 'image/png');
		assert.match(calls[0].url, /client-one\/avatar$/);
		assert.match(calls[1].url, /client-one\/avatar$/);
		assert.match(calls[2].url, /client-one\/avatar$/);
		assert.equal(calls.every(call => new Headers(call.init.headers).get('Authorization') === 'Bearer fixture'), true);
	} finally { globalThis.fetch = original; }
});

test('avatar transport treats a missing avatar as null', async () => {
	const original = globalThis.fetch;
	try {
		globalThis.fetch = async () => new Response(null, { status: 404 });
		assert.equal(await getClientAvatar('client-missing', new AbortController().signal), null);
	} finally { globalThis.fetch = original; }
});
