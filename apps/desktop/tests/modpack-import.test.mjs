import assert from 'node:assert/strict';
import { test, after } from 'node:test';
import { readFile, writeFile, mkdtemp, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';

// Exercise the actual rune-compiled window state with an isolated Runtime adapter.
const root = fileURLToPath(new URL('../../../', import.meta.url));
await mkdir(resolve(root, 'target'), { recursive: true });
const directory = await mkdtemp(resolve(root, 'target/modpack-ui-test-'));
after(() => rm(directory, { recursive: true, force: true }));
const adapter = resolve(directory, 'runtime.mjs');
await writeFile(adapter, `
export const api = { operations: [], calls: [], import: null };
export class RuntimeRequestError extends Error {}
export const listClientInstallOperations = async () => api.operations;
export const getClientInstallOperation = async id => api.operations.find(op => op.id === id);
export const importClientModpack = (...args) => { api.calls.push(args); return api.import(...args); };
export const cancelClientInstall = async id => { const op=api.operations.find(op=>op.id===id); const ended={...op,state:'cancelled',cancelRequested:false}; api.operations=[ended];return ended; };
export const startClientInstall = async () => { throw new Error('must not install a plain version'); };
export const pauseClientInstall = async () => { throw new Error('no import pause'); };
export const resumeClientInstall = pauseClientInstall;
`);
const source = await readFile(resolve(root, 'apps/desktop/src/lib/client-install-state.svelte.ts'), 'utf8');
const javascript = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const compiled = compileModule(javascript, { filename: 'client-install-state.svelte.js', generate: 'client' }).js.code
  .replace(/import \{ createContext \} from ['"]svelte['"];?/, 'const createContext = () => [() => null, value => value];')
  .replace(/from ['"]\.\/runtime['"]/g, `from ${JSON.stringify(pathToFileURL(adapter).href)}`);
await writeFile(resolve(directory, 'state.mjs'), compiled);
const { api } = await import(pathToFileURL(adapter).href);
const { ClientInstallState } = await import(pathToFileURL(resolve(directory, 'state.mjs')).href);
const pack = () => new File(['fixture'], '海岛生存.mrpack');
function operation(stage = 'uploading', state = 'running') {
  return { id: 1, name: '海岛生存', versionId: '26.1.2', loader: null, importRequestId: 'request', state, cancelRequested: false,
    progress: { stage, downloadedBytes: 0, totalBytes: null, receivedBytes: 0, totalFiles: null, completedFiles: 0, javaMajor: null, javaStage: null }, result: null, error: null };
}
async function ready(onCompleted) { api.operations=[];api.calls=[];const state=new ClientInstallState(onCompleted);await state.sync();return state; }

test('selected file creates one global task and suppresses pause while parsing', async () => {
  const state=await ready();let release;
  api.import=async (file,key,signal,onReserved) => { const op={...operation(),importRequestId:key};api.operations=[op];onReserved(op);await new Promise(resolve=>release=resolve);return {...op,progress:{...op.progress,stage:'parsing'}}; };
  try {
    const work=state.importPack(pack());
    assert.equal(state.starting,true);assert.equal(state.importing,true);assert.equal(state.active,true);assert.equal(state.canPause,false);
    release();await work;
    assert.equal(state.operation.progress.stage,'parsing');assert.equal(state.uncertain,false);assert.equal(api.calls.length,1);
  } finally {state.dispose();}
});

test('lost upload response recovers accepted task without resubmitting', async () => {
  const state=await ready();api.import=async (file,key) => {api.operations=[{...operation('parsing'),importRequestId:key}];throw new Error('lost response');};
  try {await state.importPack(pack());assert.equal(state.operation.id,1);assert.equal(state.active,true);assert.equal(state.uncertain,false);assert.equal(api.calls.length,1);} finally {state.dispose();}
});

test('retry keeps the selected file and completion refreshes the client library once', async () => {
  const completed=[];const state=await ready(receipt=>completed.push(receipt));const file=pack();
  const failed=operation('parsing','failed');api.operations=[failed];await state.sync();state.importFile=file;
  const receipt={id:'client-pack-example',name:'海岛生存',versionId:'26.1.2',loader:null,alreadyInstalled:false};
  api.import=async (selected,key) => {assert.equal(selected,file);const done={...operation('completed','completed'),importRequestId:key,result:receipt};api.operations=[done];return done;};
  try {assert.equal(state.canRetry,true);await state.retry();assert.equal(api.calls.length,1);assert.equal(completed.length,1);assert.equal(state.importFile,null);await state.sync();assert.equal(completed.length,1);} finally {state.dispose();}
});

test('cancel during upload cancels the retained backend task', async () => {
  const state=await ready();api.import=async (file,key,signal,onReserved) => {const op={...operation(),importRequestId:key};api.operations=[op];onReserved(op);await new Promise((_,reject)=>signal.addEventListener('abort',()=>reject(new Error('aborted')),{once:true}));};
  try {const work=state.importPack(pack());await state.cancel();await work;assert.equal(state.operation.state,'cancelled');assert.equal(state.canRetry,true);} finally {state.dispose();}
});

test('reload needs file selection before retry; invalid files never reach Runtime', async () => {
  const state=await ready();api.operations=[operation('parsing','failed')];await state.sync();assert.equal(state.canRetry,false);
  try {await state.importPack(new File(['x'],'other.zip'));assert.equal(api.calls.length,0);assert.match(state.error,/mrpack/);} finally {state.dispose();}
});
