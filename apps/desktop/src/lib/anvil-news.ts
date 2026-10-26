/** Bundled editorial cards. A future Anvil feed can supply the same presentation fields. */
export const anvilNews = [
	{
		id: 'your-next-world', category: 'Anvil 动态', title: '下一段冒险，\n从这里出发。',
		description: '把版本、模组与存档各自安放，随时回到熟悉的世界。',
		action: '打开实例库', href: '/clients', artwork: 'journey',
	},
	{
		id: 'make-it-yours', category: '功能发现', title: '一点新玩法，\n很多种可能。',
		description: '发现适合你的模组，为每一个实例搭配不同的玩法。',
		action: '发现模组', href: '/mods', artwork: 'blocks',
	},
	{
		id: 'a-different-light', category: '光影体验', title: '换束光，\n重看这个世界。',
		description: '从一款光影开始，为熟悉的风景添上新的色彩。',
		action: '浏览光影', href: '/shaders', artwork: 'landscape',
	},
] as const;

export type AnvilNewsArtwork = typeof anvilNews[number]['artwork'];
