import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readModDiscoveryContext, updateModDiscoveryContext } from '../src/lib/mod-discovery-context.ts';

const url = (search = '') => new URL(`http://localhost/mods${search}`);

test('nav entry has no implicit target and instance links normalize their explicit target', () => {
	const global = readModDiscoveryContext(url());
	assert.equal(global.hasInstance, false);
	assert.equal(global.instanceName, null);
	assert.equal(global.source, 'modrinth');
	assert.equal(readModDiscoveryContext(url('?instance=26.3-Fabric')).instanceName, '26.3-fabric');
});

test('ambiguous or unsafe target links cannot silently select another instance', () => {
	for (const search of ['?instance=', '?instance=../game', '?instance=game.', '?instance=one&instance=two']) {
		const context = readModDiscoveryContext(url(search));
		assert.equal(context.hasInstance, true);
		assert.equal(context.instanceName, null);
		assert.ok(context.instanceIssue);
	}
});

test('invalid and duplicate sources stay unresolved until explicitly corrected', () => {
	for (const search of ['?source=', '?source=unknown', '?source=modrinth&source=curseforge']) {
		const current = url(search);
		assert.equal(readModDiscoveryContext(current).source, null);
		const next = updateModDiscoveryContext(current, { source: 'modrinth' });
		assert.equal(readModDiscoveryContext(next).source, 'modrinth');
		assert.equal(next.searchParams.getAll('source').length, 0);
	}
});

test('changing a target invalidates the selected project while keeping search and source', () => {
	const current = url('?instance=first&source=curseforge&q=fabric+api&project=42&index=40');
	const next = updateModDiscoveryContext(current, { instance: 'Second' });
	assert.equal(next.searchParams.get('instance'), 'second');
	assert.equal(next.searchParams.get('q'), 'fabric api');
	assert.equal(next.searchParams.get('source'), 'curseforge');
	assert.equal(next.searchParams.has('project'), false);
	assert.equal(next.searchParams.has('index'), false);
	assert.equal(current.searchParams.get('project'), '42');
	assert.equal(updateModDiscoveryContext(current, { instance: 'FIRST' }).searchParams.get('project'), '42');
	assert.equal(updateModDiscoveryContext(current, { instance: 'FIRST' }).searchParams.get('index'), '40');
});

test('clearing an invalid or selected target returns to explicit global browsing', () => {
	for (const search of ['?instance=first&project=abcdefgh&q=sodium', '?instance=&project=abcdefgh&q=sodium', '?instance=one&instance=two&project=abcdefgh&q=sodium']) {
		const next = updateModDiscoveryContext(url(search), { instance: null });
		assert.equal(next.searchParams.has('instance'), false);
		assert.equal(next.searchParams.has('project'), false);
		assert.equal(next.searchParams.get('q'), 'sodium');
	}
});

test('switching providers clears provider-specific project identity and preserves the target', () => {
	const next = updateModDiscoveryContext(url('?instance=first&project=abcdefgh&q=sodium'), { source: 'curseforge' });
	assert.equal(next.searchParams.get('source'), 'curseforge');
	assert.equal(next.searchParams.has('project'), false);
	assert.equal(next.searchParams.get('instance'), 'first');
	assert.equal(next.searchParams.get('q'), 'sodium');
	assert.throws(() => updateModDiscoveryContext(next, { instance: '../escape' }));
});
