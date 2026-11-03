export default {
	"home": "首页",
	"clients": "客户端",
	"servers": "服务端",
	"mods": "模组",
	"shaders": "光影",
	"settings": "设置",
	"mainNavigation": "主导航",
	"instance": "实例",
	sidebarAction: (expanded: boolean, area: 'mods' | 'shaders' | 'clients') => {
		const labels = {"mods": "模组筛选侧栏", "shaders": "光影筛选侧栏", "clients": "实例侧栏"};
		return `${expanded ? '收起' : '展开'}${labels[area]}`;
	},

};
