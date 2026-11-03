// Shell recovery can have a different origin from the App. Do not read App storage.
(() => {
	let locale = 'zh-CN';
	const languages = navigator.languages.length ? navigator.languages : [navigator.language];
	for (const language of languages) {
		const primary = language.toLowerCase().split('-')[0];
		if (primary === 'en') { locale = 'en'; break; }
		if (primary === 'zh') { locale = 'zh-CN'; break; }
	}
	document.documentElement.lang = locale;
	document.documentElement.dataset.recoveryLocale = locale;
	document.title = locale === 'en' ? 'Anvil — Recovery' : 'Anvil — 恢复';
	document.addEventListener('DOMContentLoaded', () => {
		document.getElementById('retry')?.addEventListener('click', () => location.reload());
	}, { once: true });
})();
