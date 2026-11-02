import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { developmentEnv } from './lib/development-env.mjs';

let env;
try {
	env = developmentEnv();
	delete env.ANVIL_BUNDLE_CREDENTIALS;
} catch (error) {
	console.error(error.message);
	process.exit(1);
}
const child = spawn(process.execPath, [
	fileURLToPath(new URL('../node_modules/@tauri-apps/cli/tauri.js', import.meta.url)),
	'dev', '--config', 'src-tauri/tauri.dev.conf.json', ...process.argv.slice(2)
], {
	cwd: fileURLToPath(new URL('../apps/shell', import.meta.url)),
	stdio: 'inherit',
	env
});
for (const signal of ['SIGINT', 'SIGTERM']) {
	process.on(signal, () => child.kill(signal));
}
child.on('error', () => {
	console.error('Cannot start the desktop development launcher.');
	process.exitCode = 1;
});
child.on('exit', (code, signal) => {
	process.exitCode = code ?? (signal === 'SIGINT' ? 130 : 1);
});
