import assert from 'node:assert/strict';
import test from 'node:test';
import { supportsShellCapability, shellUpgradeRequired, shellUpgradeMessage } from '../src/lib/shell-compatibility.ts';

test('old Shell keeps known baseline features but cannot claim new permissions', () => {
	const old = { shellApiVersion: '1.0.0' };
	assert.equal(supportsShellCapability(old, 'window.theme'), true);
	assert.equal(supportsShellCapability(old, 'window.effects'), false);
	assert.equal(supportsShellCapability(null, 'window.theme'), false);
	assert.equal(supportsShellCapability({ shellApiVersion: '9.0.0' }, 'window.theme'), false);
});

test('explicit capabilities are authoritative and missing features fail closed', () => {
	assert.equal(supportsShellCapability({ shellApiVersion: '1.0.0', capabilities: [] }, 'window.theme'), false);
	assert.equal(supportsShellCapability({ shellApiVersion: '1.1.0', capabilities: ['window.theme'] }, 'window.effects'), false);
	assert.equal(supportsShellCapability({ shellApiVersion: '1.1.0', capabilities: ['window.effects'] }, 'window.effects'), true);
});

test('a damaged package is never mislabeled as a required Shell upgrade', () => {
	assert.equal(shellUpgradeRequired({ failure: 'invalid_package' }), false);
	assert.equal(shellUpgradeRequired(null), false);
	assert.equal(shellUpgradeRequired({ failure: 'shell_upgrade_required' }), true);
	assert.match(shellUpgradeMessage({ availableVersion: '26.1.0' }), /Desktop 26.1.0/);
	assert.match(shellUpgradeMessage({ availableVersion: null }), /当前版本仍可继续使用/);
});
