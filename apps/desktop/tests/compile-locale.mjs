// Test-only compiler for the real locale singleton and its TS dictionaries.
import { readdir, readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve, relative } from 'node:path';
import { pathToFileURL } from 'node:url';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';
export async function compileLocale(root, directory) {
	const sourceRoot = resolve(root, 'apps/desktop/src/lib/i18n');
	const output = resolve(directory, 'i18n');
	async function visit(folder) {
		for (const entry of await readdir(folder, { withFileTypes: true })) {
			const path = resolve(folder, entry.name);
			if (entry.isDirectory()) { await visit(path); continue; }
			if (!entry.name.endsWith('.ts')) continue;
			const target = resolve(output, relative(sourceRoot, path).replace(/\.ts$/, '.mjs'));
			await mkdir(resolve(target, '..'), { recursive: true });
			const js = ts.transpileModule(await readFile(path, 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText.replace(/from ('[^']+)\.ts'/g, 'from $1.mjs\'');
			await writeFile(target, js);
		}
	}
	await visit(sourceRoot);
	const helperSource = await readFile(resolve(root, 'apps/desktop/src/lib/runtime-errors.ts'), 'utf8');
	const helper = ts.transpileModule(helperSource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText.replace("'./i18n/index.ts'", "'./i18n/index.mjs'");
	await writeFile(resolve(directory, 'runtime-errors.mjs'), helper);
	const source = await readFile(resolve(root, 'apps/desktop/src/lib/locale.svelte.ts'), 'utf8');
	const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
	const compiled = compileModule(js, { filename: 'locale.svelte.js', generate: 'client' }).js.code.replace("'./i18n/index.ts'", "'./i18n/index.mjs'");
	const path = resolve(directory, 'locale.mjs');
	await writeFile(path, compiled);
	return pathToFileURL(path).href;
}
