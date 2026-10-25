import { createContext } from 'svelte';

/** Window-owned preferences; each page area retains its own expanded state. */
export type PageSidebarArea = 'clients' | 'mods' | 'shaders';
export class PageSidebarState {
	clients = $state(true);
	mods = $state(true);
	shaders = $state(true);
	toggle(area: PageSidebarArea) { this[area] = !this[area]; }
}
export const [getPageSidebarState, setPageSidebarState] = createContext<PageSidebarState>();
