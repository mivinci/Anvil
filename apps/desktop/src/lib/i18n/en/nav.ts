import type { Messages } from '../zh-CN.ts';

export default {
	"home": "Home",
	"clients": "Instances",
	"servers": "Servers",
	"mods": "Mods",
	"shaders": "Shaders",
	"settings": "Settings",
	"mainNavigation": "Main navigation",
	"instance": "Instance",
	sidebarAction: (expanded: boolean, area: 'mods' | 'shaders' | 'clients') => {
		const labels = {"mods": "mod filters", "shaders": "shader filters", "clients": "instance sidebar"};
		return `${expanded ? 'Hide' : 'Show'} ${labels[area]}`;
	},

} satisfies Messages['nav'];
