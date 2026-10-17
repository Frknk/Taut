# web

Panel de taut: SvelteKit 5 + Tailwind 4, compilado como SPA estática (adapter-static con
`fallback: index.html`). Se gestiona con bun; Vite corre sobre Node 24.

```bash
bun install          # dependencias
bun run dev          # http://localhost:5173 (necesita la api en http://localhost:8080)
bun run check        # svelte-check
bun run lint         # oxlint (solo el <script> de los .svelte)
bun run fmt          # oxfmt (los .svelte pasan por prettier-plugin-svelte)
bunx playwright install chromium   # una vez: los tests de componentes corren en Chromium
bun run test         # vitest: lógica en Node y componentes (*.svelte.test.ts) con axe
bun run build        # build/ listo para servir desde un CDN
```

`PUBLIC_API_URL` (ver `.env.example`) fija la URL de la api al construir.
