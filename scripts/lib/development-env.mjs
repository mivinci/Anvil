import { readFileSync } from 'node:fs';
import { parseEnv } from 'node:util';
import { fileURLToPath } from 'node:url';

const repositoryEnv = fileURLToPath(new URL('../../.env', import.meta.url));

// Development only; explicit process variables take precedence over .env.
export function developmentEnv(env = process.env, envFile = repositoryEnv) {
	let local;
	try {
		local = parseEnv(readFileSync(envFile, 'utf8'));
	} catch (error) {
		if (error.code === 'ENOENT') return { ...env };
		throw new Error('Cannot load the project development .env file.');
	}
	return { ...local, ...env };
}
