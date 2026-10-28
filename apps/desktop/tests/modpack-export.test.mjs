import assert from 'node:assert/strict';
import { test, after } from 'node:test';
import { readFile, writeFile, mkdtemp, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';

const root = fileURLToPath(new URL('../../../', import.meta.url));
await mkdir(resolve(root, 'target'), { recursive: true });
const directory = await mkdtemp(resolve(root, 'target/modpack-export-ui-test-'));
after(() => rm(directory, { recursive: true, force: true }));
const adapter = resolve(directory, 'runtime.mjs');
await writeFile(adapter, `
export const api = { exports: [], calls: [], create: null };
export class RuntimeRequestError extends Error {}
export const createModpackExport = (...args) => { api.calls.push(args); return api.create(...args); };
export const listModpackExports = async () => api.exports;
export const cancelModpackExport = async id => { const op = api.exports.find(op => op.id === id); const ended = { ...op, state: 'cancelled', cancelRequested: false }; api.exports = [ended]; return ended; };
`);
const source = await readFile(resolve(root, 'apps/desktop/src/lib/modpack-export-state.svelte.ts'), 'utf8');
const javascript = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const compiled = compileModule(javascript, { filename: 'modpack-export-state.svelte.js', generate: 'client' }).js.code
  .replace(/import \{ createContext \} from ['"]svelte['"];?/, 'const createContext = () => [() => null, value => value];')
  .replace(/from ['"]\.\/runtime['"]/g, `from ${JSON.stringify(pathToFileURL(adapter).href)}`);
await writeFile(resolve(directory, 'state.mjs'), compiled);
const { api } = await import(pathToFileURL(adapter).href);
const { ModpackExportState } = await import(pathToFileURL(resolve(directory, 'state.mjs')).href);
const options = { name: '海岛生存', versionId: '1.0.0', selection: { mods: true, resourcePacks: true, shaderPacks: true, config: true, worlds: false } };
function operation(state = 'running', stage = 'collecting', requestId = 'request') {
  return { id: 1, requestId, name: 'test', state, cancelRequested: false,
    progress: { stage, completedFiles: 0, totalFiles: null, processedBytes: 0, totalBytes: null },
    result: null, error: null };
}
async function ready() { api.exports = []; api.calls = []; const state = new ModpackExportState(); await state.sync(); return state; }

test('starts one background export and exposes its progress', async () => {
  const state = await ready();
  api.create = async (name, requestId) => { const op = operation('running', 'packing', requestId); api.exports = [op]; return op; };
  try {
    assert.equal(await state.start('test', options), true);
    assert.equal(state.active, true);
    assert.equal(state.label, '正在打包整合包…');
    assert.equal(api.calls.length, 1);
  } finally { state.dispose(); }
});

test('lost create response recovers the accepted task by request id', async () => {
  const state = await ready();
  api.create = async (_name, requestId) => { api.exports = [operation('running', 'collecting', requestId)]; throw new Error('lost response'); };
  try {
    assert.equal(await state.start('test', options), false);
    assert.equal(state.uncertain, true);
    await state.sync();
    assert.equal(state.operation.id, 1);
    assert.equal(state.uncertain, false);
    assert.equal(api.calls.length, 1);
  } finally { state.dispose(); }
});

test('completed export keeps the download path and can be cancelled while running', async () => {
  const state = await ready();
  api.create = async (_name, requestId) => { const op = operation('running', 'publishing', requestId); api.exports = [op]; return op; };
  try {
    await state.start('test', options);
    await state.cancel();
    assert.equal(state.operation.state, 'cancelled');
    const done = { ...operation('completed', 'completed', 'done'), result: { path: '/tmp/Pack.mrpack', size: 123, sha256: 'a'.repeat(64), files: 2 } };
    api.exports = [done];
    await state.sync();
    state.operation = done;
    assert.equal(state.label, '整合包导出完成');
    assert.equal(state.operation.result.path, '/tmp/Pack.mrpack');
  } finally { state.dispose(); }
});
