<script lang="ts">
  import { afterNavigate, goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { DropdownMenu } from 'bits-ui';
  import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
  import BellIcon from 'phosphor-svelte/lib/BellIcon';
  import BroadcastIcon from 'phosphor-svelte/lib/BroadcastIcon';
  import CaretUpDownIcon from 'phosphor-svelte/lib/CaretUpDownIcon';
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
  import GearIcon from 'phosphor-svelte/lib/GearIcon';
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon';
  import MoonIcon from 'phosphor-svelte/lib/MoonIcon';
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon';
  import PulseIcon from 'phosphor-svelte/lib/PulseIcon';
  import ShieldCheckIcon from 'phosphor-svelte/lib/ShieldCheckIcon';
  import SignOutIcon from 'phosphor-svelte/lib/SignOutIcon';
  import SunIcon from 'phosphor-svelte/lib/SunIcon';
  import UserIcon from 'phosphor-svelte/lib/UserIcon';
  import UsersIcon from 'phosphor-svelte/lib/UsersIcon';
  import WarningIcon from 'phosphor-svelte/lib/WarningIcon';

  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import Menu from '#lib/components/Menu.svelte';
  import MenuItem from '#lib/components/MenuItem.svelte';
  import MenuRadioItem from '#lib/components/MenuRadioItem.svelte';
  import TenantForm from '#lib/components/TenantForm.svelte';
  import { request } from '#lib/api.js';
  import { ROLE_LABELS } from '#lib/format.js';
  import { session } from '#lib/session.svelte.js';
  import { sectionAfterSwitch } from '#lib/tenant-nav.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import { theme, type Theme } from '#lib/theme.svelte.js';
  import type { Incident, TenantWithRole } from '#lib/types.js';
  import { badgeClass, focusRing, menu, navItemClass } from '#lib/ui.js';

  /** The panel's navigation: tenant switcher, sections, and the user's menu at the bottom. */

  interface Props {
    /** In the drawer the close button sits over the header's top-right corner: leave room for it. */
    drawer?: boolean;
  }

  let { drawer = false }: Props = $props();

  const current = $derived(selectedTenant.current);

  const monitoring = [
    { href: resolve('/(app)/monitors'), label: 'Monitores', icon: PulseIcon },
    { href: resolve('/(app)/incidents'), label: 'Incidentes', icon: WarningIcon },
    { href: resolve('/(app)/channels'), label: 'Canales', icon: BellIcon },
  ];

  // API keys are for owners only: members would just see a refusal.
  const organization = $derived([
    { href: resolve('/(app)/settings/members'), label: 'Miembros', icon: UsersIcon },
    { href: resolve('/(app)/settings/tenant'), label: 'Ajustes', icon: GearIcon },
    ...(selectedTenant.isOwner
      ? [{ href: resolve('/(app)/settings/api-keys'), label: 'API keys', icon: KeyIcon }]
      : []),
  ]);

  const themes: { value: Theme; label: string; icon: typeof SunIcon }[] = [
    { value: 'system', label: 'Sistema', icon: DesktopIcon },
    { value: 'light', label: 'Claro', icon: SunIcon },
    { value: 'dark', label: 'Oscuro', icon: MoonIcon },
  ];

  function isCurrent(href: string): boolean {
    return page.url.pathname.startsWith(href);
  }

  function initial(text: string | undefined): string {
    return (text?.trim().charAt(0) ?? '?').toUpperCase();
  }

  // Open incidents of the selected tenant, beside «Incidentes».
  let openIncidents = $state(0);

  async function countOpen(tenantId: string) {
    try {
      const open = await request<Incident[]>('GET', '/v1/incidents?status=open', {
        tenant: tenantId,
      });
      if (selectedTenant.current?.id === tenantId) openIncidents = open.length;
    } catch {
      // The counter is a hint: on failure it keeps the last value and the page shows the error.
    }
  }

  $effect(() => {
    const tenantId = selectedTenant.current?.id;
    if (!tenantId) return;
    openIncidents = 0;
    void countOpen(tenantId);
    const timer = setInterval(() => void countOpen(tenantId), 30_000);
    return () => clearInterval(timer);
  });

  // Opening or closing an incident page should not wait 30 s to show in the counter.
  afterNavigate(() => {
    if (selectedTenant.current) void countOpen(selectedTenant.current.id);
  });

  function refreshTenants() {
    selectedTenant.refresh().catch(() => {
      // The menu keeps the last list; the next opening tries again.
    });
  }

  async function switchTo(tenantId: string) {
    const tenant = selectedTenant.list.find((t) => t.id === tenantId);
    if (!tenant || tenant.id === current?.id) return;
    selectedTenant.switching = true;
    selectedTenant.select(tenant);
    try {
      await goto(sectionAfterSwitch(page.url.pathname));
    } finally {
      selectedTenant.switching = false;
    }
  }

  let creatingTenant = $state(false);
  let createOpen = $state(false);

  function created(tenant: TenantWithRole) {
    createOpen = false;
    selectedTenant.select(tenant);
    refreshTenants();
    void goto(resolve('/(app)/monitors'));
  }

  const rowTrigger = 'flex w-full items-center gap-2 rounded-lg p-1.5 text-left hover:bg-tint';
  const groupTitle = 'px-2 pb-1 text-xs font-medium text-subtle';
</script>

<div class="flex h-full flex-col gap-4 overflow-y-auto p-3">
  <!-- Header row, framed like the account footer: the mark and the tenant switcher side by side. -->
  <div class={['flex items-center gap-2 border-b border-hairline pb-3', drawer && 'pe-8']}>
    <a
      href={resolve('/(app)/monitors')}
      class={['flex h-[26px] shrink-0 items-center rounded-sm px-1.5 text-strong', focusRing]}
      aria-label="Taut, inicio"
    >
      <Logo class="w-6" />
    </a>

    <Menu label="Organizaciones" triggerClass={rowTrigger} onopen={refreshTenants}>
      {#snippet trigger()}
        <span class="min-w-0 flex-1">
          <span class="sr-only">Organización:</span>
          <span class="block truncate font-medium text-strong">{current?.name}</span>
          <span class="block text-xs text-subtle">{current ? ROLE_LABELS[current.role] : ''}</span>
        </span>
        <CaretUpDownIcon size={16} class="shrink-0 text-subtle" aria-hidden="true" />
      {/snippet}

      <DropdownMenu.RadioGroup
        value={current?.id}
        onValueChange={(id) => void switchTo(id)}
        class="max-h-72 overflow-y-auto"
      >
        <DropdownMenu.GroupHeading class={menu.heading}
          >Tus organizaciones</DropdownMenu.GroupHeading
        >
        {#if !selectedTenant.loaded}
          <MenuItem disabled>Cargando…</MenuItem>
        {/if}
        {#each selectedTenant.list as tenant (tenant.id)}
          <MenuRadioItem value={tenant.id} textValue={tenant.name}>
            <span
              class="flex size-5 shrink-0 items-center justify-center rounded-sm bg-fill text-xs font-semibold text-default"
              aria-hidden="true"
            >
              {initial(tenant.name)}
            </span>
            <span class="min-w-0 flex-1 truncate">{tenant.name}</span>
            <span class="text-xs text-subtle">{ROLE_LABELS[tenant.role]}</span>
          </MenuRadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
      <DropdownMenu.Separator class={menu.separator} />
      <MenuItem onSelect={() => (createOpen = true)}>
        <PlusIcon size={16} aria-hidden="true" />
        Crear organización
      </MenuItem>
    </Menu>
  </div>

  <nav class="flex flex-col gap-4" aria-label="Secciones">
    <div>
      <h2 class={groupTitle}>Monitoreo</h2>
      <ul class="space-y-0.5">
        {#each monitoring as link (link.href)}
          {@const here = isCurrent(link.href)}
          <li>
            <a href={link.href} aria-current={here ? 'page' : undefined} class={navItemClass(here)}>
              <link.icon size={16} aria-hidden="true" />
              <span class="flex-1">{link.label}</span>
              {#if link.href === resolve('/(app)/incidents') && openIncidents > 0}
                <span class={badgeClass('danger')}>
                  {openIncidents}<span class="sr-only">
                    {openIncidents === 1 ? 'abierto' : 'abiertos'}</span
                  >
                </span>
              {/if}
            </a>
          </li>
        {/each}
      </ul>
    </div>

    <div>
      <h2 class={groupTitle}>Organización</h2>
      <ul class="space-y-0.5">
        {#each organization as link (link.href)}
          {@const here = isCurrent(link.href)}
          <li>
            <a href={link.href} aria-current={here ? 'page' : undefined} class={navItemClass(here)}>
              <link.icon size={16} aria-hidden="true" />
              {link.label}
            </a>
          </li>
        {/each}
        {#if current}
          <li>
            <a
              href={resolve('/status/[slug]', { slug: current.slug })}
              target="_blank"
              rel="noopener"
              class={navItemClass(false)}
            >
              <BroadcastIcon size={16} aria-hidden="true" />
              <span class="flex-1">Página de estado</span>
              <ArrowUpRightIcon size={14} aria-hidden="true" />
              <span class="sr-only">(se abre en otra pestaña)</span>
            </a>
          </li>
        {/if}
      </ul>
    </div>
  </nav>

  <div class="mt-auto border-t border-hairline pt-3">
    <Menu label="Cuenta" triggerClass={rowTrigger} side="top">
      {#snippet trigger()}
        <span
          class="flex size-8 shrink-0 items-center justify-center rounded-full bg-fill text-sm font-semibold text-default"
          aria-hidden="true"
        >
          {initial(session.user?.name)}
        </span>
        <span class="min-w-0 flex-1">
          <span class="sr-only">Cuenta:</span>
          <span class="block truncate font-medium text-default">{session.user?.name}</span>
          <span class="block truncate text-xs text-subtle">{session.user?.email}</span>
        </span>
        <CaretUpDownIcon size={16} class="shrink-0 text-subtle" aria-hidden="true" />
      {/snippet}

      <MenuItem href={resolve('/(app)/settings/profile')}>
        <UserIcon size={16} aria-hidden="true" />
        Perfil
      </MenuItem>
      <MenuItem href={resolve('/(app)/settings/security')}>
        <ShieldCheckIcon size={16} aria-hidden="true" />
        Seguridad
      </MenuItem>
      <DropdownMenu.Separator class={menu.separator} />
      <DropdownMenu.RadioGroup
        value={theme.current}
        onValueChange={(value) => theme.set(value as Theme)}
      >
        <DropdownMenu.GroupHeading class={menu.heading}>Tema</DropdownMenu.GroupHeading>
        {#each themes as option (option.value)}
          <!-- Stays open: trying the themes one after another is the point. -->
          <MenuRadioItem value={option.value} closeOnSelect={false}>
            <option.icon size={16} aria-hidden="true" />
            {option.label}
          </MenuRadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
      <DropdownMenu.Separator class={menu.separator} />
      <MenuItem onSelect={() => void session.logout()}>
        <SignOutIcon size={16} aria-hidden="true" />
        Salir
      </MenuItem>
    </Menu>
  </div>
</div>

<Dialog
  bind:open={createOpen}
  title="Crear una organización"
  description="Tendrá sus propios monitores, canales y miembros, y tú quedarás como propietario."
>
  <TenantForm id="sidebar-new-tenant" bind:creating={creatingTenant} oncreated={created} />
  {#snippet actions()}
    <Button onclick={() => (createOpen = false)}>Cancelar</Button>
    <Button type="submit" form="sidebar-new-tenant" variant="primary" disabled={creatingTenant}>
      {creatingTenant ? 'Creando…' : 'Crear y entrar'}
    </Button>
  {/snippet}
</Dialog>
