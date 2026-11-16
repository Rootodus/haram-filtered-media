export interface Settings {
  globalEnabled: boolean;
  allowlist: string[];
}

const DEFAULT_SETTINGS: Settings = {
  globalEnabled: true,
  allowlist: [],
};

export async function getSettings(): Promise<Settings> {
  const data = await browser.storage.local.get('settings');
  return { ...DEFAULT_SETTINGS, ...data.settings };
}

export async function saveSettings(settings: Settings): Promise<void> {
  await browser.storage.local.set({ settings });
}
