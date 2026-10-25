import { test } from 'node:test';
import assert from 'node:assert/strict';
import { modProjectIcon } from '../src/lib/mod-project-icon.ts';

const project = { id: 'abcdefgh', name: 'Fixture', summary: '', authors: [], links: { websiteUrl: null } };
test('legacy project metadata without artwork uses the placeholder', () => {
	assert.equal(modProjectIcon(project, 'modrinth'), null);
	assert.equal(modProjectIcon(project, 'curseforge'), null);
});
test('each provider uses its own artwork and CurseForge prefers thumbnails', () => {
	assert.equal(modProjectIcon({ ...project, iconUrl: 'https://cdn.modrinth.com/icon.png' }, 'modrinth'), 'https://cdn.modrinth.com/icon.png');
	assert.equal(modProjectIcon({ ...project, logo: { thumbnailUrl: 'https://media.forgecdn.net/small.png', url: 'https://edge.forgecdn.net/full.png' } }, 'curseforge'), 'https://media.forgecdn.net/small.png');
	assert.equal(modProjectIcon({ ...project, logo: { thumbnailUrl: null, url: 'https://edge.forgecdn.net/full.png' } }, 'curseforge'), 'https://edge.forgecdn.net/full.png');
});
test('invalid artwork cannot become an image request', () => {
	for (const iconUrl of ['javascript:alert(1)', 'http://cdn.modrinth.com/icon.png', 'https://cdn.modrinth.com.evil.invalid/icon.png', 'https://user@cdn.modrinth.com/icon.png', 'https://cdn.modrinth.com:8443/icon.png', 'https://media.forgecdn.net/icon.png', 'x'.repeat(2049)]) {
		assert.equal(modProjectIcon({ ...project, iconUrl }, 'modrinth'), null);
	}
});
