import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { signRuntime, signingConfiguration } from './lib/runtime-signing.mjs';

const identity = 'AB'.repeat(20);
test('unconfigured builds keep the existing signature', () => {
	assert.equal(signingConfiguration({}), null);
});
test('rejects ambiguous, ad-hoc and incomplete identity configuration', () => {
	for (const value of ['-', 'Anvil Development', '123']) {
		assert.throws(() => signingConfiguration({ ANVIL_MACOS_SIGNING_IDENTITY: value }));
	}
	assert.throws(() => signingConfiguration({ ANVIL_MACOS_SIGNING_KEYCHAIN: '/tmp/keychain' }));
});
test('other platforms never invoke codesign', () => {
	signRuntime('/tmp/runtime', 'x86_64-pc-windows-msvc', {}, () => assert.fail('codesign invoked'));
});
test('Runtime signature pins the identifier and certificate', { skip: process.platform !== 'darwin' }, () => {
	const calls = [];
	signRuntime('/tmp/runtime', 'aarch64-apple-darwin', {
		ANVIL_MACOS_SIGNING_IDENTITY: identity,
		ANVIL_MACOS_SIGNING_KEYCHAIN: '/tmp/private.keychain-db'
	}, (command, args) => {
		calls.push({ command, args });
		return { status: 0 };
	});
	assert.equal(calls.length, 2);
	assert.ok(calls[0].args.includes('app.q2q.anvil.runtime'));
	assert.ok(calls[0].args.includes('/tmp/private.keychain-db'));
	assert.ok(calls[1].args.includes(`=identifier "app.q2q.anvil.runtime" and certificate leaf = H"${identity}"`));
});
test('configured signing errors fail the build without an ad-hoc fallback', { skip: process.platform !== 'darwin' }, () => {
	let count = 0;
	assert.throws(() => signRuntime('/tmp/runtime', 'aarch64-apple-darwin', {
		ANVIL_MACOS_SIGNING_IDENTITY: identity
	}, () => { count++; return { status: 1 }; }));
	assert.equal(count, 1);
});

test('macOS release configuration preserves the application bundle identity', () => {
	const config = (name) => JSON.parse(readFileSync(new URL(`../apps/shell/src-tauri/${name}`, import.meta.url), 'utf8'));
	const base = config('tauri.conf.json');
	const macos = config('tauri.macos.conf.json');
	const development = config('tauri.dev.conf.json');
	for (const configuration of [base, macos, development]) {
		assert.ok(Object.hasOwn(configuration, 'identifier'), 'Missing required config field: identifier');
		assert.match(configuration.identifier, /^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/);
	}
	assert.equal(macos.identifier, 'app.q2q.anvil');
	assert.equal(macos.identifier, base.identifier);
	assert.equal({ ...base, ...macos }.identifier, 'app.q2q.anvil');
	assert.equal({ ...base, ...macos, ...development }.identifier, 'app.q2q.anvil.dev');
});
