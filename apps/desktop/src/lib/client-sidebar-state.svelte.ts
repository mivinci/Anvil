import { createContext } from 'svelte';

/** Window-owned preference; changing detail tabs does not reset the sidebar. */
export class ClientSidebarState {
	open = $state(true);
	toggle() { this.open = !this.open; }
}
export const [getClientSidebarState, setClientSidebarState] = createContext<ClientSidebarState>();
