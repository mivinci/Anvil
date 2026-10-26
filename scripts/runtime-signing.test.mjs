import assert from 'node:assert/strict';
import test from 'node:test';
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
