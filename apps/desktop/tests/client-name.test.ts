import assert from 'node:assert/strict';
import test from 'node:test';
import { validClientName } from '../src/lib/client-name.ts';
import { readModDiscoveryContext, updateModDiscoveryContext } from '../src/lib/mod-discovery-context.ts';

test('accepts imported Unicode names and applies Core byte limits', () => {
	for (const name of ['海岛生存', '海岛生存-2', 'Café', 'a'.repeat(48), '界'.repeat(42)]) assert.equal(validClientName(name), true, name);
	for (const name of ['', '../海岛生存', '海岛/生存', '海岛\\生存', '海岛生存.', '海岛 生存', 'a'.repeat(49), '界'.repeat(43)]) assert.equal(validClientName(name), false, name);
});

test('retains Unicode instance target when navigating to mod discovery', () => {
	const next = updateModDiscoveryContext(new URL('https://anvil.test/mods'), { instance: '海岛生存-2' });
	const context = readModDiscoveryContext(next);
	assert.equal(context.instanceName, '海岛生存-2');
	assert.equal(context.instanceIssue, null);
	assert.equal(readModDiscoveryContext(new URL('https://anvil.test/mods?instance=%E6%B5%B7%E5%B2%9B%E7%94%9F%E5%AD%98')).instanceName, '海岛生存');
});
