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
  const saved = data.settings as Partial<Settings> | undefined;
  
  return { 
    ...DEFAULT_SETTINGS, 
    ...saved 
  };
}

export async function saveSettings(settings: Settings): Promise<void> {
  await browser.storage.local.set({ settings });
}
