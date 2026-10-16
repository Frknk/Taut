<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import type { Snippet } from 'svelte';

  import { focusRing } from '#lib/ui.js';

  let { children }: { children: Snippet } = $props();

  const sections = [
    { href: resolve('/(app)/settings/profile'), label: 'Perfil' },
    { href: resolve('/(app)/settings/security'), label: 'Seguridad' },
    { href: resolve('/(app)/settings/members'), label: 'Miembros' },
  ];
</script>

<h1 class="text-2xl font-semibold text-strong">Configuración</h1>
<p class="mt-0.5 text-[13px]/[18px] text-subtle">Tu cuenta y la organización en la que estás.</p>

<nav class="mt-6 flex gap-1 border-b border-hairline" aria-label="Secciones de configuración">
  {#each sections as section (section.href)}
    {@const current = page.url.pathname.startsWith(section.href)}
    <a
      href={section.href}
      aria-current={current ? 'page' : undefined}
      class={[
        '-mb-px inline-flex h-9 items-center rounded-t-md border-b-2 px-3 text-sm',
        focusRing,
        current
          ? 'border-contrast font-medium text-default'
          : 'border-transparent text-subtle hover:text-default',
      ]}
    >
      {section.label}
    </a>
  {/each}
</nav>

<div class="mt-6 space-y-6">
  {@render children()}
</div>
