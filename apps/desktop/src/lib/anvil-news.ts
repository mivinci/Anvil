import { messages, type Locale } from './i18n/index.ts';

const cards = [
	{
		"id": "your-next-world",
		"href": "/clients",
		"artwork": "journey"
	},
	{
		"id": "make-it-yours",
		"href": "/mods",
		"artwork": "blocks"
	},
	{
		"id": "a-different-light",
		"href": "/shaders",
		"artwork": "landscape"
	}
] as const;
export const getAnvilNews = (locale: Locale) => cards.map((card, index) => ({ ...card, ...messages[locale].home.editorial[index]! }));
export type AnvilNewsArtwork = typeof cards[number]['artwork'];
