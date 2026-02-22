import { initPolyfill, PolyfillConfig } from '../../polyfill/src/core';

// Get asset URLs from the Chrome extension
const config: PolyfillConfig = {
  wasmUrl: chrome.runtime.getURL('vm-rust/pkg/vm_rust_bg.wasm'),
  systemFontUrl: chrome.runtime.getURL('charmap-system.png'),
};

// Initialize the polyfill with extension version for priority negotiation
const version = chrome.runtime.getManifest().version;
initPolyfill(config, version, 'extension');
