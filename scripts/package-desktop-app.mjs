import { signRuntime } from './lib/runtime-signing.mjs';
import { spawnSync } from 'node:child_process';
import { chmod, copyFile, lstat, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const shellProject = path.join(repositoryRoot, 'apps/shell/src-tauri');
const desktopProject = path.join(repositoryRoot, 'apps/desktop');
const target = process.env.TAURI_ENV_TARGET_TRIPLE ?? getRustHostTriple();
const manifestPath = process.env.ANVIL_APP_MANIFEST
	? path.resolve(shellProject, process.env.ANVIL_APP_MANIFEST)
	: path.join(desktopProject, 'manifests', `${target}.json`);
const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
const expectedAppId = process.env.ANVIL_DEFAULT_APP_ID ?? 'anvil.desktop';

if (manifest.app_id !== expectedAppId || manifest.target !== target) {
	throw new Error(`App manifest ${manifestPath} does not match ${expectedAppId} for ${target}`);
}
validateEntry(manifest.page_entry, 'web/');
validateEntry(manifest.sidecar_entry, 'runtime/');

run('npm', ['run', 'build', '--workspace', '@anvil/desktop']);
run('cargo', ['build', '--locked', '--release', '--package', 'anvil', '--target', target]);

const binaryName = process.platform === 'win32' || target.includes('windows') ? 'anvil.exe' : 'anvil';
const cargoTargetDirectory = path.resolve(repositoryRoot, process.env.CARGO_TARGET_DIR ?? 'target');
const sidecarPath = path.join(cargoTargetDirectory, target, 'release', binaryName);
const pageDirectory = path.join(desktopProject, 'build');
const baselineDirectory = path.join(desktopProject, 'baseline');

if (!(await isFile(path.join(pageDirectory, 'index.html')))) {
	throw new Error(`SvelteKit build did not produce ${path.join(pageDirectory, 'index.html')}`);
}
if (!(await isFile(sidecarPath))) {
	throw new Error(`Cargo did not produce the target sidecar ${sidecarPath}`);
}

await removeGeneratedDirectory(baselineDirectory);
await mkdir(path.join(baselineDirectory, 'web'), { recursive: true });
await mkdir(path.dirname(path.join(baselineDirectory, manifest.sidecar_entry)), { recursive: true });
await copyTree(pageDirectory, path.join(baselineDirectory, 'web'));
await copyFile(sidecarPath, path.join(baselineDirectory, manifest.sidecar_entry));
await chmod(
	path.join(baselineDirectory, manifest.sidecar_entry),
	(await stat(sidecarPath)).mode
);
signRuntime(path.join(baselineDirectory, manifest.sidecar_entry), target);
await writeFile(path.join(baselineDirectory, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);

console.log(`Packaged ${manifest.app_id} ${manifest.version} for ${target} at ${baselineDirectory}`);

function run(command, args) {
	const result = spawnSync(command, args, {
		cwd: repositoryRoot,
		stdio: 'inherit',
		env: process.env,
		shell: process.platform === 'win32'
	});
	if (result.error) throw result.error;
	if (result.status !== 0) process.exit(result.status ?? 1);
}

function getRustHostTriple() {
	const result = spawnSync('rustc', ['-vV'], { cwd: repositoryRoot, encoding: 'utf8' });
	if (result.status !== 0 || result.error) {
		throw new Error(`Unable to determine Rust host target: ${result.error ?? result.stderr}`);
	}
	const host = result.stdout.match(/^host: (.+)$/m)?.[1];
	if (!host) throw new Error('rustc -vV did not report a host target');
	return host;
}

async function isFile(filePath) {
	try {
		return (await lstat(filePath)).isFile();
	} catch (error) {
		if (error.code === 'ENOENT') return false;
		throw error;
	}
}

async function removeGeneratedDirectory(directory) {
	try {
		const metadata = await lstat(directory);
		if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
			throw new Error(`Refusing to replace non-directory generated path ${directory}`);
		}
		await rm(directory, { recursive: true });
	} catch (error) {
		if (error.code !== 'ENOENT') throw error;
	}
}

async function copyTree(source, destination) {
	for (const entry of await readdir(source, { withFileTypes: true })) {
		const sourcePath = path.join(source, entry.name);
		const destinationPath = path.join(destination, entry.name);
		const metadata = await lstat(sourcePath);
		if (metadata.isSymbolicLink()) throw new Error(`Build output contains a symbolic link: ${sourcePath}`);
		if (metadata.isDirectory()) {
			await mkdir(destinationPath, { recursive: true });
			await copyTree(sourcePath, destinationPath);
		} else if (metadata.isFile()) {
			await copyFile(sourcePath, destinationPath);
			await chmod(destinationPath, metadata.mode & 0o777);
		} else {
			throw new Error(`Build output contains an unsupported file: ${sourcePath}`);
		}
	}
}

function validateEntry(entry, expectedPrefix) {
	if (
		typeof entry !== 'string' ||
		!entry.startsWith(expectedPrefix) ||
		entry.includes('\\') ||
		entry.includes(':') ||
		entry.split('/').some((part) => !part || part === '.' || part === '..')
	) {
		throw new Error(`App manifest contains an invalid package entry: ${entry}`);
	}
}
