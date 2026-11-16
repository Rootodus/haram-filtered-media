import './style.css';
import { getSettings, saveSettings } from '@/utils/storage';

document.addEventListener('DOMContentLoaded', async () => {
  const globalToggle = document.getElementById('global-toggle') as HTMLInputElement;
  const siteToggle = document.getElementById('site-toggle') as HTMLInputElement;
  const domainLabel = document.getElementById('current-domain')!;
  const statusBadge = document.getElementById('server-status')!;
  const statusText = document.getElementById('status-text')!;

  // 1. Check Server Status (ping the Rust /health endpoint)
  async function checkServerStatus() {
    try {
      const res = await fetch('http://127.0.0.1:8377/health', { signal: AbortSignal.timeout(1000) });
      if (res.ok) {
        statusBadge.classList.replace('offline', 'online');
        statusText.textContent = 'Server running';
        return;
      }
    } catch (e) {
      // Ignored, we just fall back to offline state
    }
    statusBadge.classList.replace('online', 'offline');
    statusText.textContent = 'Server offline';
  }

  // 2. Load current tab URL and settings
  const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
  let currentDomain = '';
  
  if (tab?.url && tab.url.startsWith('http')) {
    try {
      currentDomain = new URL(tab.url).hostname;
      domainLabel.textContent = currentDomain;
    } catch (e) {
      domainLabel.textContent = 'Invalid URL';
    }
  } else {
    domainLabel.textContent = 'Not a web page';
    siteToggle.disabled = true;
  }

  const settings = await getSettings();
  
  globalToggle.checked = settings.globalEnabled;
  
  if (currentDomain) {
    siteToggle.checked = settings.allowlist.includes(currentDomain);
  }

  if (!settings.globalEnabled) {
    siteToggle.disabled = true;
  }

  // 3. Event Listeners
  globalToggle.addEventListener('change', async () => {
    settings.globalEnabled = globalToggle.checked;
    await saveSettings(settings);
    
    siteToggle.disabled = !globalToggle.checked;
    
    // Notify background to reload content scripts if needed
    browser.runtime.sendMessage({ action: 'settings_changed' });
  });

  siteToggle.addEventListener('change', async () => {
    if (!currentDomain) return;

    if (siteToggle.checked) {
      if (!settings.allowlist.includes(currentDomain)) {
        settings.allowlist.push(currentDomain);
      }
    } else {
      settings.allowlist = settings.allowlist.filter(d => d !== currentDomain);
    }
    
    await saveSettings(settings);
    browser.runtime.sendMessage({ action: 'settings_changed' });
  });

  // Run server check
  checkServerStatus();
});
