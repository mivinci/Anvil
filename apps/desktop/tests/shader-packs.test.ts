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

test('shared ZIP validation exposes stable issues and locale-specific text without changing limits', async () => {
	const { localPackFileIssue, localPackSize } = await import('../src/lib/local-pack.ts');
	const { shaderEnvironmentLabels } = await import('../src/lib/shader-packs.ts');
	const invalid = { name: 'empty.zip', size: 0 };
	assert.equal(localPackFileIssue(invalid), 'invalidZip');
	assert.match(localPackFileError(invalid, 'en')!, /non-empty ZIP/);
	assert.match(localPackFileError(invalid, 'zh-CN')!, /非空/);
	assert.equal(localPackFileIssue({ name: 'con.zip', size: 1 }), 'invalidZipName');
	assert.equal(localPackSize(1024, 'en'), '1.0 KiB');
	assert.equal(localPackSize(1024 ** 2, 'en'), '1.0 MiB');
	assert.equal(shaderEnvironmentLabels('en').missing_iris, 'Iris is not installed');
	assert.equal(shaderEnvironmentLabels('zh-CN').ready, '光影支持已就绪');
});

test('structured shader reasons translate parameters without parsing legacy descriptions', async () => {
	const { shaderEnvironmentReason } = await import('../src/lib/shader-packs.ts');
	const env = { status: 'missing_dependencies' as const, managedOnly: true, iris: { version: '1', enabled: true }, sodium: null, reason: 'arbitrary source wording', reasonCode: 'shader_missing_dependency', reasonParams: { mod: 'iris', dependency: 'sodium' } };
	assert.equal(shaderEnvironmentReason(env, 'en'), 'iris: A required enabled dependency is missing: sodium.');
	assert.match(shaderEnvironmentReason(env, 'zh-CN'), /iris.*必需依赖.*sodium/);
	assert.match(shaderEnvironmentReason({ ...env, reasonCode: 'future_reason' }, 'en'), /unconfirmed/);
	assert.match(shaderEnvironmentReason({ ...env, reasonCode: undefined, reasonParams: undefined }, 'en'), /required managed mods/);
	assert.equal(env.reason, 'arbitrary source wording');
});
