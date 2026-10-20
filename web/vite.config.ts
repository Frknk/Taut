import { defineConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';
import tailwindcss from '@tailwindcss/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
  // PUBLIC_API_URL, PUBLIC_AUTH_URL y PUBLIC_STATUS_URL (src/lib/api.ts) se leen con
  // import.meta.env, que por defecto solo expone VITE_*.
  envPrefix: ['VITE_', 'PUBLIC_'],
  plugins: [
    tailwindcss(),
    sveltekit({
      compilerOptions: {
        // Force runes mode for the project, except for libraries. Can be removed in svelte 6.
        runes: ({ filename }) =>
          filename.split(/[/\\]/).includes('node_modules') ? undefined : true,
      },
      // SPA estática: toda ruta no prerenderizada se sirve desde index.html (docs/adr/0005).
      adapter: adapter({ fallback: 'index.html' }),
    }),
  ],
  test: {
    expect: { requireAssertions: true },
    projects: [
      {
        extends: './vite.config.ts',
        test: {
          name: 'server',
          environment: 'node',
          include: ['src/**/*.{test,spec}.{js,ts}'],
          exclude: ['src/**/*.svelte.{test,spec}.{js,ts}'],
        },
      },
      {
        // Components run in a real Chromium: focus, portals and keyboard behave as in the app.
        extends: './vite.config.ts',
        test: {
          name: 'client',
          include: ['src/**/*.svelte.{test,spec}.{js,ts}'],
          setupFiles: ['src/lib/test/setup.ts'],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: 'chromium' }],
          },
        },
      },
    ],
  },
});
