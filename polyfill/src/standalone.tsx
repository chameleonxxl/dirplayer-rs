import { initPolyfill } from './core';
import { getEmbeddedWasmUrl, getEmbeddedFontUrl } from './embedded-loader';

declare const DIRPLAYER_VERSION: string;

declare global {
  interface Window {
    DirPlayer: {
      init: () => void;
    };
  }
}

function init() {
  const config = {
    wasmUrl: getEmbeddedWasmUrl(),
    systemFontUrl: getEmbeddedFontUrl(),
  };

  // Register with version for priority negotiation (deferred init handled inside initPolyfill)
  initPolyfill(config, DIRPLAYER_VERSION, 'polyfill');
}

// Expose the API globally
window.DirPlayer = {
  init,
};

// Auto-initialize unless data-manual-init is present on the script tag
const currentScript = document.currentScript as HTMLScriptElement | null;
if (!currentScript?.hasAttribute('data-manual-init')) {
  init();
}
