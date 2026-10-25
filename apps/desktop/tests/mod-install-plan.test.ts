import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ModInstallPlanState, type ModInstallPlan, type ModInstallFile, type ModPlanSnapshot } from '../src/lib/mod-install-plan.ts';

const context = { source: 'modrinth' as const, name: 'first', gameVersion: '26.3', project: 'abcdefgh' };
const file = (id = 'versiona'): ModInstallFile => ({ id, displayName: id, fileName: `${id}.jar`, fileLength: 100, fileDate: '2026-10-24T00:00:00Z', releaseType: 1 });
function plan(selected = file(), id = 1): ModInstallPlan {
	return { planId: id, expiresInMs: 600000, revision: 1, gameVersion: '26.3', totalBytes: 100, downloadBytes: 100, optionalProjects: [], warnings: [],
		items: [{ project: { id: 'abcdefgh', name: 'Fixture', summary: '', authors: [], links: { websiteUrl: null } }, file: selected, direct: true }],
		files: [{ projectId: 'abcdefgh', fileId: selected.id, projectName: 'Fixture', filename: selected.fileName, size: 100, direct: true, reuse: false }] };
}
function harness(overrides: Partial<ConstructorParameters<typeof ModInstallPlanState>[0]> = {}) {
	let value: ModPlanSnapshot;
	const starts: { name: string; id: number }[] = [];
	const state = new ModInstallPlanState({ read: async (_context, selected) => plan(selected), start: async (name, id) => { starts.push({ name, id }); return true; }, canSubmit: () => true, ...overrides }, next => { value = next; });
	state.bind(context);
	return { state, starts, get value() { return value!; } };
}

test('changing versions aborts the old read and ignores its late response', async () => {
	const pending: { signal: AbortSignal; finish: (value: ModInstallPlan) => void }[] = [];
	const h = harness({ read: async (_context, _file, signal) => new Promise(resolve => { pending.push({ signal, finish: resolve }); }) });
	const first = h.state.select(file('versiona'));
	const second = h.state.select(file('versionb'));
	assert.equal(pending[0]!.signal.aborted, true);
	pending[1]!.finish(plan(file('versionb'), 2)); await second;
	pending[0]!.finish(plan(file('versiona'), 1)); await first;
	assert.equal(h.value.plan?.planId, 2);
	assert.equal(h.value.selected?.id, 'versionb');
	h.state.dispose();
});

test('changing source, target, game version or project invalidates the old plan', async () => {
	for (const change of [{ name: 'second' }, { source: 'curseforge' as const }, { gameVersion: '1.21.1' }, { project: 'ijklmnop' }]) {
		const h = harness(); await h.state.select(file());
		h.state.bind({ ...context, ...change });
		assert.equal(h.value.plan, null);
		assert.equal(await h.state.confirm(), false);
		assert.equal(h.starts.length, 0);
		h.state.dispose();
	}
});

test('plan identity and game version must match the selected target and file', async () => {
	for (const invalid of [{ ...plan(), gameVersion: '1.21.1' }, plan(file('another1')), { ...plan(), planId: 0 }]) {
		const h = harness({ read: async () => invalid }); await h.state.select(file());
		assert.equal(h.value.plan, null); assert.ok(h.value.error);
		assert.equal(await h.state.confirm(), false); h.state.dispose();
	}
});

test('expiry includes request time and is rechecked when confirming after sleep', async () => {
	let now = 0;
	const h = harness({ now: () => now, read: async () => { now = 100; return { ...plan(), expiresInMs: 1000 }; } });
	await h.state.select(file()); now = 1001;
	assert.equal(await h.state.confirm(), false);
	assert.equal(h.value.expired, true); assert.equal(h.starts.length, 0); h.state.dispose();
});

test('double confirmation submits once and accepted tasks survive page disposal', async () => {
	let finish!: (value: boolean) => void;
	const starts: { name: string; id: number }[] = [];
	const h = harness({ start: async (name, id) => { starts.push({ name, id }); return new Promise(resolve => { finish = resolve; }); } });
	await h.state.select(file());
	const submitted = h.state.confirm(); assert.equal(await h.state.confirm(), false);
	h.state.dispose(); finish(true);
	assert.equal(await submitted, true);
	assert.deepEqual(starts, [{ name: 'first', id: 1 }]);
	assert.equal(h.value.accepted, false);
});

test('unconfirmed submission retries the same plan while accepted plans cannot resubmit', async () => {
	const ids: number[] = [];
	const h = harness({ start: async (_name, id) => { ids.push(id); return ids.length > 1; } });
	await h.state.select(file()); assert.equal(await h.state.confirm(), false);
	assert.equal(await h.state.confirm(), true); assert.equal(await h.state.confirm(), false);
	assert.deepEqual(ids, [1, 1]); assert.equal(h.value.accepted, true); h.state.dispose();
});

test('window task state can block submission without losing the reviewed plan', async () => {
	const h = harness({ canSubmit: () => false }); await h.state.select(file());
	assert.equal(await h.state.confirm(), false); assert.equal(h.starts.length, 0);
	assert.equal(h.value.plan?.planId, 1); h.state.dispose();
});

test('CurseForge numeric project and file identities use the same explicit confirmation flow', async () => {
	const selected = { ...file(), id: 7 };
	const value = plan(selected, 4);
	value.items[0]!.project.id = 42;
	value.files[0]!.projectId = 42;
	const h = harness({ read: async () => value });
	h.state.bind({ ...context, source: 'curseforge', project: '42' });
	await h.state.select(selected);
	assert.equal(h.value.error, null);
	assert.equal(await h.state.confirm(), true);
	assert.deepEqual(h.starts, [{ name: 'first', id: 4 }]);
	h.state.dispose();
});
