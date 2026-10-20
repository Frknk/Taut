<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import type { Snippet } from 'svelte';

  import { selectedTenant } from '#lib/tenant.svelte.js';

  let { children }: { children: Snippet } = $props();

  // The sidebar links each settings page on its own: the header tells which one this is.
  const org = $derived(selectedTenant.current?.name ?? 'la organización');
  const headers = $derived([
    {
      href: resolve('/(app)/settings/profile'),
      title: 'Perfil',
      text: 'Tu nombre, tu correo y tu contraseña.',
    },
    {
      href: resolve('/(app)/settings/security'),
      title: 'Seguridad',
      text: 'Verificación en dos pasos y códigos de respaldo de tu cuenta.',
    },
    {
      href: resolve('/(app)/settings/members'),
      title: 'Miembros',
      text: `Quién tiene acceso a ${org} y con qué rol.`,
    },
    {
      href: resolve('/(app)/settings/tenant'),
      title: 'Ajustes de la organización',
      text: `Plan y nombre de ${org}, salir de ella o eliminarla.`,
    },
    {
      href: resolve('/(app)/settings/api-keys'),
      title: 'API keys',
      text: `Claves para usar la API de monitores e incidentes de ${org} desde scripts.`,
    },
  ]);
  const header = $derived(headers.find((h) => page.url.pathname.startsWith(h.href)));
</script>

{#if header}
  <h1 class="text-2xl font-semibold text-strong">{header.title}</h1>
  <p class="mt-0.5 text-[13px]/[18px] text-subtle">{header.text}</p>
{/if}

<div class="mt-6 space-y-6">
  {@render children()}
</div>
