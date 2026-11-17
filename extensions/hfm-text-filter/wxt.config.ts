import { defineConfig } from 'wxt';

export default defineConfig({
  manifestVersion: 3,
  manifest: ({ browser }) => ({
    name: 'HFM Text Filter',
    permissions: ['activeTab', 'scripting', 'storage'],
    host_permissions: [
      'http://127.0.0.1:8377/*',
      'ws://127.0.0.1:8377/*'
    ],

    ...(browser === 'firefox' && {
      browser_specific_settings: {
        gecko: {
          id: 'hfm-text-filter@rootodus',
          data_collection_permissions: {
            required: ['none'],
          },
        },
      },
    }),
  }),
});
