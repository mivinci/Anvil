import { test } from 'node:test';
import assert from 'node:assert/strict';
import { localPackFileError, MAX_LOCAL_PACK_BYTES } from '../src/lib/local-pack.ts';
import { validateShaderPacks, validateShaderPackChange, refreshAfterShaderChange } from '../src/lib/shader-packs.ts';
import type { ClientShaderPacks } from '../src/lib/runtime.ts';

const snapshot = (revision = 1): ClientShaderPacks => ({
	revision, appliedRevision: revision,
	packs: [{ source: 'local', filename: 'Light.zip', size: 100, sha256: 'a'.repeat(64) }],
	environment: { status: 'missing_iris', iris: null, sodium: null, managedOnly: true, reason: '尚未安装 Iris。' },
});

test('shader ZIP selection rejects oversized, empty and unsafe local filenames', () => {
	for (const file of [
		{ name: 'Light.zip', size: 0 }, { name: 'Light.zip', size: MAX_LOCAL_PACK_BYTES + 1 },
		{ name: '../Light.zip', size: 10 }, { name: 'CON.zip', size: 10 }, { name: 'COM1.zip', size: 10 },
		{ name: '光影.zip', size: 10 }, { name: 'Light.jar', size: 10 },
	]) assert.ok(localPackFileError(file), JSON.stringify(file));
	assert.equal(localPackFileError({ name: 'Light + Test.ZIP', size: MAX_LOCAL_PACK_BYTES }), null);
});

test('shader response validation rejects duplicate identities and inconsistent revisions', () => {
	const value = snapshot();
	assert.equal(validateShaderPacks(value), value);
	assert.throws(() => validateShaderPackChange({ ...value, appliedRevision: 2 }));
	assert.throws(() => validateShaderPackChange({ ...value, packs: [...value.packs, ...value.packs] }));
	assert.throws(() => validateShaderPackChange({ ...value, packs: [...value.packs, { ...value.packs[0], sha256: 'b'.repeat(64), filename: 'light.ZIP' }] }));
	assert.throws(() => validateShaderPackChange({ ...value, packs: [{ ...value.packs[0], size: NaN }] }));
});

test('shader readiness cannot be fabricated from a missing or disabled Iris', () => {
	const value = snapshot();
	assert.throws(() => validateShaderPacks({ ...value, environment: { ...value.environment, status: 'ready' } }));
	assert.throws(() => validateShaderPacks({ ...value, environment: { ...value.environment, status: 'ready', iris: { version: '1.8.8', enabled: false } } }));
	assert.throws(() => validateShaderPacks({ ...value, environment: { ...value.environment, managedOnly: false } }));
	assert.equal(validateShaderPacks({ ...value, environment: { ...value.environment, status: 'unconfirmed' } }).environment.status, 'unconfirmed');
});

test('a confirmed shader write remains confirmed when the environment read fails', async () => {
	const committed = snapshot(3);
	const result = await refreshAfterShaderChange(committed, async () => { throw new Error('offline'); }, new AbortController().signal);
	assert.equal(result.snapshot, null);
	assert.equal(result.error, 'offline');
	assert.equal(committed.revision, 3);
	assert.equal(committed.packs.length, 1);
});

test('post-write refresh accepts newer snapshots and rejects older or malformed reads', async () => {
	const signal = new AbortController().signal;
	assert.equal((await refreshAfterShaderChange(snapshot(3), async () => snapshot(4), signal)).snapshot?.revision, 4);
	const stale = await refreshAfterShaderChange(snapshot(3), async () => snapshot(2), signal);
	assert.equal(stale.snapshot, null);
	assert.match(stale.error ?? '', /旧的光影清单/);
	const invalid = await refreshAfterShaderChange(snapshot(3), async () => ({ ...snapshot(3), environment: null! }), signal);
	assert.equal(invalid.snapshot, null);
	assert.match(invalid.error ?? '', /环境/);
});

test('leaving a shader page aborts the post-write read instead of publishing an old instance', async () => {
	const controller = new AbortController();
	await assert.rejects(refreshAfterShaderChange(snapshot(3), async () => {
		controller.abort();
		return snapshot(3);
	}, controller.signal), { name: 'AbortError' });
});
