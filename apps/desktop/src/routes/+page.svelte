<script lang="ts">
	import {
		ArrowUpRight,
		Box,
		ChevronDown,
		CircleHelp,
		Gamepad2,
		Hammer,
		Plus,
		Search,
		Server,
		Settings2,
		Sparkles,
		UserRound,
	} from '@lucide/svelte';
	import Button from '#lib/components/ui/button.svelte';

	const navigation = [
		{ label: '客户端', icon: Gamepad2, active: true },
		{ label: '服务端', icon: Server, active: false },
		{ label: '模组与整合包', icon: Box, active: false },
	];
</script>

<svelte:head>
	<title>Anvil — Minecraft Java</title>
	<meta
		name="description"
		content="Anvil 桌面端基础界面，后续通过 Tauri 调用共享 Rust Core。"
	/>
</svelte:head>

<div class="app-shell">
	<aside class="sidebar">
		<div class="brand-row">
			<div class="brand-mark"><Hammer size={18} strokeWidth={2.4} /></div>
			<div>
				<div class="brand-name">anvil</div>
				<div class="brand-caption">MINECRAFT JAVA</div>
			</div>
		</div>

		<div class="profile-switcher">
			<div class="profile-avatar"><UserRound size={16} /></div>
			<div class="profile-copy">
				<strong>本地玩家</strong>
				<span>尚未登录</span>
			</div>
			<ChevronDown size={15} class="subtle-icon" />
		</div>

		<div class="nav-label">游戏</div>
		<nav aria-label="主导航">
			{#each navigation as item}
				{@const Icon = item.icon}
				<button class:active={item.active} class="nav-item" disabled={!item.active}>
					<Icon size={17} strokeWidth={1.8} />
					<span>{item.label}</span>
			</button>
			{/each}
		</nav>

		<div class="sidebar-spacer"></div>
		<div class="sidebar-footer">
			<button class="nav-item"><Settings2 size={17} strokeWidth={1.8} /><span>设置</span></button>
			<button class="nav-item"><CircleHelp size={17} strokeWidth={1.8} /><span>帮助</span></button>
			<div class="sidebar-version">桌面端 · 开发中</div>
		</div>
	</aside>

	<main class="main-panel">
		<header class="topbar">
			<div class="breadcrumb"><span>游戏</span><span class="crumb-divider">/</span><strong>客户端</strong></div>
			<div class="topbar-actions">
				<div class="search-field"><Search size={15} /><span>搜索</span><kbd>⌘ K</kbd></div>
				<Button variant="ghost" size="icon" aria-label="账户"><UserRound size={17} /></Button>
			</div>
		</header>

		<div class="page-content">
			<section class="page-heading">
				<div>
					<div class="eyebrow"><Sparkles size={13} /> LAUNCHER</div>
					<h1>你的游戏库</h1>
					<p>管理原版 Minecraft 客户端和启动配置。</p>
				</div>
				<Button disabled>
					<Plus size={16} />
					安装新版本
				</Button>
			</section>

			<section class="welcome-card" aria-labelledby="welcome-title">
				<div class="welcome-art" aria-hidden="true">
					<div class="orb orb-back"></div>
					<div class="orb orb-front"></div>
					<div class="welcome-cube"><Box size={46} strokeWidth={1.2} /></div>
				</div>
				<div class="welcome-copy">
					<div class="welcome-tag">ANVIL DESKTOP</div>
					<h2 id="welcome-title">准备好开始冒险了吗？</h2>
					<p>桌面端界面已搭好基础结构，接下来会把安装、账户和启动流程接入共享 Rust Core。</p>
					<div class="welcome-actions">
						<Button disabled><Plus size={16} />安装 Minecraft</Button>
						<Button variant="outline" disabled>了解项目 <ArrowUpRight size={15} /></Button>
					</div>
				</div>
			</section>

			<section class="section-heading">
				<div>
					<h2>最近游玩</h2>
					<p>安装的客户端会显示在这里</p>
				</div>
				<Button variant="ghost" size="sm" disabled>查看全部 <ArrowUpRight size={14} /></Button>
			</section>

			<section class="empty-card">
				<div class="empty-icon"><Gamepad2 size={21} strokeWidth={1.7} /></div>
				<strong>还没有客户端</strong>
				<span>安装一个 Minecraft 版本后，就可以从这里启动。</span>
				<Button variant="outline" size="sm" disabled><Plus size={14} />添加客户端</Button>
			</section>

			<footer class="main-footer">
				<span><span class="status-dot"></span> Core 尚未连接</span>
				<span>本地桌面预览</span>
			</footer>
		</div>
	</main>
</div>

<style>
	.app-shell { display: flex; min-height: 100vh; background: #101113; }
	.sidebar { width: 246px; flex: 0 0 246px; display: flex; flex-direction: column; padding: 25px 15px 18px; border-right: 1px solid #ffffff0d; background: #131416; }
	.brand-row { display: flex; align-items: center; gap: 11px; padding: 0 9px 25px; }
	.brand-mark { display: grid; width: 34px; height: 34px; place-items: center; border-radius: 10px; color: #122119; background: #a3e6b3; }
	.brand-name { font-size: 17px; font-weight: 750; letter-spacing: -.04em; line-height: 1.1; }
	.brand-caption { margin-top: 4px; color: #72757c; font-size: 8px; font-weight: 700; letter-spacing: .15em; }
	.profile-switcher { display: flex; align-items: center; gap: 10px; margin-bottom: 28px; padding: 10px; border: 1px solid #ffffff0d; border-radius: 11px; background: #191a1d; }
	.profile-avatar { display: grid; width: 32px; height: 32px; flex: 0 0 32px; place-items: center; border-radius: 9px; color: #b6bac3; background: #292b30; }
	.profile-copy { display: grid; flex: 1; gap: 3px; }
	.profile-copy strong { font-size: 12px; font-weight: 600; }
	.profile-copy span { color: #777b83; font-size: 11px; }
	.nav-label { padding: 0 10px 9px; color: #777b83; font-size: 10px; font-weight: 650; letter-spacing: .09em; text-transform: uppercase; }
	nav, .sidebar-footer { display: grid; gap: 4px; }
	.nav-item { display: flex; align-items: center; gap: 11px; width: 100%; border: 0; border-radius: 9px; padding: 10px; color: #9b9ea5; background: transparent; text-align: left; font-size: 12px; }
	.nav-item.active { color: #e4e7e5; background: #ffffff0d; }
	.nav-item:disabled { opacity: .55; }
	.sidebar-spacer { flex: 1; }
	.sidebar-footer { padding-top: 14px; border-top: 1px solid #ffffff0d; }
	.sidebar-version { padding: 15px 10px 0; color: #696d74; font-size: 10px; }
	.main-panel { min-width: 0; flex: 1; }
	.topbar { display: flex; height: 66px; align-items: center; justify-content: space-between; padding: 0 38px; border-bottom: 1px solid #ffffff0d; }
	.breadcrumb, .topbar-actions { display: flex; align-items: center; gap: 11px; }
	.breadcrumb { color: #858991; font-size: 12px; }
	.breadcrumb strong { color: #e7e8eb; font-weight: 550; }
	.crumb-divider { color: #4f5258; }
	.topbar-actions { gap: 12px; }
	.search-field { display: flex; width: 185px; align-items: center; gap: 8px; color: #737780; font-size: 11px; }
	kbd { margin-left: auto; padding: 3px 5px; border: 1px solid #ffffff13; border-radius: 5px; color: #737780; font: inherit; font-size: 9px; }
	.page-content { max-width: 1160px; margin: 0 auto; padding: 40px 42px 28px; }
	.page-heading, .section-heading { display: flex; align-items: center; justify-content: space-between; }
	.eyebrow { display: flex; align-items: center; gap: 6px; margin-bottom: 9px; color: #a3e6b3; font-size: 9px; font-weight: 700; letter-spacing: .12em; }
	h1 { margin: 0; font-size: 27px; font-weight: 650; letter-spacing: -.04em; }
	.page-heading p, .section-heading p { margin: 8px 0 0; color: #82858d; font-size: 12px; }
	.welcome-card { position: relative; display: flex; min-height: 236px; align-items: center; overflow: hidden; margin-top: 31px; padding: 29px 37px; border: 1px solid #ffffff12; border-radius: 18px; background: radial-gradient(ellipse at 85% 44%, #254d3a5c 0, transparent 39%), linear-gradient(105deg, #1c1f1e, #17191b 68%); }
	.welcome-copy { position: relative; z-index: 1; max-width: 520px; }
	.welcome-tag { color: #8fce9f; font-size: 9px; font-weight: 700; letter-spacing: .15em; }
	h2 { margin: 11px 0 0; font-size: 21px; font-weight: 620; letter-spacing: -.025em; }
	.welcome-copy p { max-width: 450px; margin: 9px 0 0; color: #9da19f; font-size: 12px; line-height: 1.7; }
	.welcome-actions { display: flex; gap: 9px; margin-top: 20px; }
	.welcome-art { position: absolute; top: 0; right: 2%; width: 42%; height: 100%; display: grid; place-items: center; color: #a3e6b3; }
	.orb { position: absolute; border: 1px solid #9fe3b427; border-radius: 50%; transform: rotate(-24deg); }
	.orb-back { width: 194px; height: 80px; }
	.orb-front { width: 155px; height: 110px; transform: rotate(55deg); }
	.welcome-cube { display: grid; width: 108px; height: 108px; place-items: center; border: 1px solid #b2f0c035; border-radius: 25px; background: linear-gradient(145deg, #314d3c, #1e2c25); box-shadow: 0 22px 60px #0008, inset 0 1px #ffffff18; transform: rotate(-8deg); }
	.section-heading { margin-top: 35px; }
	.section-heading h2 { margin: 0; font-size: 15px; letter-spacing: -.015em; }
	.section-heading p { margin-top: 5px; font-size: 11px; }
	.empty-card { display: grid; min-height: 182px; justify-items: center; align-content: center; gap: 9px; margin-top: 14px; border: 1px dashed #ffffff1c; border-radius: 14px; color: #82858d; }
	.empty-icon { display: grid; width: 42px; height: 42px; place-items: center; margin-bottom: 3px; border-radius: 13px; color: #9b9ea5; background: #ffffff0b; }
	.empty-card strong { color: #d6d8dc; font-size: 12px; font-weight: 570; }
	.empty-card span { font-size: 11px; }
	.empty-card :global(button) { margin-top: 6px; }
	.main-footer { display: flex; justify-content: space-between; margin-top: 22px; color: #696d74; font-size: 10px; }
	.main-footer > span:first-child { display: flex; align-items: center; gap: 7px; }
	.status-dot { width: 6px; height: 6px; border-radius: 50%; background: #d2a259; }
</style>
