import { readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const identifier = 'app.q2q.anvil.runtime';

// The certificate fingerprint selects a unique identity; a display name is ambiguous.
export function signingConfiguration(env = process.env) {
	const identity = env.ANVIL_MACOS_SIGNING_IDENTITY?.trim();
	const keychain = env.ANVIL_MACOS_SIGNING_KEYCHAIN?.trim();
	if (!identity) {
		if (keychain) throw new Error('Signing keychain requires ANVIL_MACOS_SIGNING_IDENTITY');
		return null;
	}
	if (!/^[a-fA-F0-9]{40}$/.test(identity)) {
		throw new Error('ANVIL_MACOS_SIGNING_IDENTITY must be the certificate SHA-1 fingerprint (40 hex characters)');
	}
	return { identity: identity.toUpperCase(), keychain };
}

export function localSigningEnvironment(env = process.env) {
	if (process.platform !== 'darwin' || env.CI || env.ANVIL_MACOS_SIGNING_IDENTITY) return env;
	const configPath = path.join(os.homedir(), 'Library/Application Support/Anvil Development Signing/identity.json');
	let config;
	try { config = JSON.parse(readFileSync(configPath, 'utf8')); }
	catch (error) { if (error.code === 'ENOENT') return env; throw error; }
	return { ...env, ANVIL_MACOS_SIGNING_IDENTITY: config.identity, ANVIL_MACOS_SIGNING_KEYCHAIN: config.keychain };
}

export function signRuntime(binary, target, env = process.env, execute = spawnSync) {
	if (!target.endsWith('-apple-darwin')) return;
	const config = signingConfiguration(localSigningEnvironment(env));
	if (!config) {
		console.log('macOS Runtime: no certificate configured; keeping existing ad-hoc signature');
		return;
	}
	if (process.platform !== 'darwin') throw new Error('macOS Runtime signing requires a macOS host');
	const invoke = (args) => {
		const result = execute('codesign', args, { encoding: 'utf8', env });
		if (result.error || result.status !== 0) {
			throw new Error('macOS Runtime certificate signing/verification failed; check the configured identity and keychain');
		}
	};
	const args = ['--force', '--sign', config.identity, '--identifier', identifier, '--timestamp=none'];
	if (config.keychain) args.push('--keychain', config.keychain);
	invoke([...args, binary]);
	invoke(['--verify', '--strict', '-R', `=identifier "${identifier}" and certificate leaf = H"${config.identity}"`, binary]);
	console.log(`macOS Runtime signed and verified: ${identifier}`);
}
