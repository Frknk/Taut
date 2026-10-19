<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import Select from '#lib/components/Select.svelte';
  import { ApiError, describeError, request } from '#lib/api.js';
  import { formatDateTime, ROLE_LABELS, timeAgo } from '#lib/format.js';
  import {
    emptyInvite,
    isExpired,
    toInviteInput,
    validateInvite,
    type InviteErrors,
  } from '#lib/invite-form.js';
  import { FLIP, leave } from '#lib/motion.js';
  import { session } from '#lib/session.svelte.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Invitation, Member, Role } from '#lib/types.js';
  import { fieldError, help, input, label, table } from '#lib/ui.js';

  const tenantId = $derived(selectedTenant.current?.id ?? '');
  const isOwner = $derived(selectedTenant.isOwner);
  const roles = Object.keys(ROLE_LABELS) as Role[];
  const roleItems = roles.map((role) => ({ value: role, label: ROLE_LABELS[role] }));

  let members = $state<Member[]>([]);
  let invitations = $state<Invitation[]>([]);
  let error = $state('');
  let notice = $state('');

  let values = $state(emptyInvite());
  let errors = $state<InviteErrors>({});
  let inviting = $state(false);

  let removing = $state<Member | null>(null);
  let confirmingRemove = $state(false);

  /** The api's 409 about the last owner, in Spanish. */
  function describe(err: unknown): string {
    if (err instanceof ApiError && err.message.includes('at least one owner')) {
      return 'La organización necesita al menos un propietario: nombra otro antes.';
    }
    return describeError(err);
  }

  async function load() {
    try {
      const [loadedMembers, loadedInvitations] = await Promise.all([
        request<Member[]>('GET', '/v1/members', { tenant: tenantId }),
        isOwner
          ? request<Invitation[]>('GET', '/v1/invitations', { tenant: tenantId })
          : Promise.resolve([]),
      ]);
      members = loadedMembers;
      invitations = loadedInvitations;
    } catch (err) {
      error = describe(err);
    }
  }

  onMount(load);

  async function changeRole(member: Member, role: Role) {
    error = '';
    notice = '';
    try {
      await request<Member>('PUT', `/v1/members/${member.user_id}`, {
        tenant: tenantId,
        body: { role },
      });
      notice = `${member.name} ahora es ${ROLE_LABELS[role].toLowerCase()}.`;
      // Stepping down: the panel stops offering what only owners can do.
      const current = selectedTenant.current;
      if (member.user_id === session.user?.id && current)
        selectedTenant.select({ ...current, role });
    } catch (err) {
      error = describe(err);
    }
    // Also on failure: the select goes back to the stored role.
    await load();
  }

  function askRemove(member: Member) {
    removing = member;
    confirmingRemove = true;
  }

  async function remove(member: Member) {
    confirmingRemove = false;
    error = '';
    notice = '';
    try {
      await request<void>('DELETE', `/v1/members/${member.user_id}`, { tenant: tenantId });
      notice = `${member.name} ya no es miembro.`;
      await load();
    } catch (err) {
      error = describe(err);
    }
  }

  async function invite(event: SubmitEvent) {
    event.preventDefault();
    errors = validateInvite(
      values,
      members.map((member) => member.email),
    );
    if (Object.keys(errors).length > 0) return;
    inviting = true;
    error = '';
    notice = '';
    try {
      const sent = await request<Invitation>('POST', '/v1/invitations', {
        tenant: tenantId,
        body: toInviteInput(values),
      });
      notice = `Enviamos la invitación a ${sent.email}.`;
      values = emptyInvite();
      await load();
    } catch (err) {
      error = describe(err);
    } finally {
      inviting = false;
    }
  }

  async function revoke(invitation: Invitation) {
    error = '';
    notice = '';
    try {
      await request<void>('DELETE', `/v1/invitations/${invitation.id}`, { tenant: tenantId });
      notice = `Revocamos la invitación de ${invitation.email}.`;
      await load();
    } catch (err) {
      error = describe(err);
    }
  }
</script>

<svelte:head>
  <title>Miembros · Taut</title>
</svelte:head>

{#if error}<Banner tone="danger">{error}</Banner>{/if}
{#if notice}<Banner tone="success">{notice}</Banner>{/if}

<LayerCard title="Miembros de {selectedTenant.current?.name}" flush>
  <table class={table.root}>
    <thead>
      <tr>
        <th class={table.th}>Nombre</th>
        <th class={table.th}>Correo</th>
        <th class={table.th}>Rol</th>
        <th class={table.th}>Se unió</th>
        {#if isOwner}<th class={table.th}><span class="sr-only">Acciones</span></th>{/if}
      </tr>
    </thead>
    <tbody>
      {#each members as member (member.user_id)}
        {@const isMe = member.user_id === session.user?.id}
        <tr class={table.row} animate:flip={FLIP} out:leave>
          <td class={[table.td, 'font-medium text-strong']}>
            {member.name}
            {#if isMe}<span class="font-normal text-subtle">(tú)</span>{/if}
          </td>
          <td class={table.td}>{member.email}</td>
          <td class={table.td}>
            {#if isOwner}
              <label class="sr-only" for="role-{member.user_id}">Rol de {member.name}</label>
              <Select
                id="role-{member.user_id}"
                inline
                value={member.role}
                items={roleItems}
                onchange={(role) => changeRole(member, role)}
              />
            {:else}
              <Badge tone={member.role === 'owner' ? 'inverted' : 'outline'}>
                {ROLE_LABELS[member.role]}
              </Badge>
            {/if}
          </td>
          <td class={[table.td, 'text-subtle']}>{timeAgo(member.joined_at)}</td>
          {#if isOwner}
            <td class={[table.td, 'text-right']}>
              {#if !isMe}
                <Button size="sm" variant="secondary-destructive" onclick={() => askRemove(member)}>
                  Quitar
                </Button>
              {/if}
            </td>
          {/if}
        </tr>
      {/each}
    </tbody>
  </table>
  {#if !isOwner}
    <p class="border-t border-hairline px-3 py-2 text-xs text-subtle">
      Solo los propietarios invitan, cambian roles y quitan miembros.
    </p>
  {/if}
</LayerCard>

<Dialog
  bind:open={confirmingRemove}
  alert
  title="¿Quitar a {removing?.name ?? ''}?"
  description="Perderá el acceso a esta organización de inmediato. Puedes volver a invitarle después."
>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirmingRemove = false)}>Cancelar</Button>
    <Button variant="destructive" onclick={() => removing && remove(removing)}>Quitar</Button>
  {/snippet}
</Dialog>

{#if isOwner}
  <form onsubmit={invite} novalidate>
    <LayerCard title="Invitar a alguien">
      <div class="space-y-4">
        <div class="grid gap-4 sm:grid-cols-[1fr_12rem]">
          <div>
            <label class={label} for="invite-email">Correo</label>
            <input
              id="invite-email"
              class={input}
              type="email"
              autocomplete="off"
              bind:value={values.email}
              placeholder="persona@empresa.com"
              aria-invalid={errors.email ? 'true' : undefined}
            />
            {#if errors.email}
              <p class={fieldError}>{errors.email}</p>
            {:else}
              <p class={help}>
                Le llega un enlace válido 7 días; tiene que entrar con este correo.
              </p>
            {/if}
          </div>
          <div>
            <label class={label} for="invite-role">Rol</label>
            <Select id="invite-role" bind:value={values.role} items={roleItems} />
            {#if errors.role}<p class={fieldError}>{errors.role}</p>{/if}
          </div>
        </div>
        <Button type="submit" variant="primary" disabled={inviting}>
          {inviting ? 'Enviando…' : 'Enviar invitación'}
        </Button>
      </div>
    </LayerCard>
  </form>

  <LayerCard title="Invitaciones pendientes" flush>
    {#if invitations.length === 0}
      <p class="px-4 py-3 text-sm text-subtle">No hay invitaciones pendientes.</p>
    {:else}
      <table class={table.root}>
        <thead>
          <tr>
            <th class={table.th}>Correo</th>
            <th class={table.th}>Rol</th>
            <th class={table.th}>Invitó</th>
            <th class={table.th}>Vence</th>
            <th class={table.th}><span class="sr-only">Acciones</span></th>
          </tr>
        </thead>
        <tbody>
          {#each invitations as invitation (invitation.id)}
            <tr class={table.row} animate:flip={FLIP} out:leave>
              <td class={[table.td, 'font-medium text-strong']}>{invitation.email}</td>
              <td class={table.td}>
                <Badge tone="outline">{ROLE_LABELS[invitation.role]}</Badge>
              </td>
              <td class={[table.td, 'text-subtle']}>{invitation.invited_by ?? '—'}</td>
              <td class={table.td}>
                {#if isExpired(invitation.expires_at)}
                  <Badge tone="warning">Vencida</Badge>
                {:else}
                  <span class="text-subtle">{formatDateTime(invitation.expires_at)}</span>
                {/if}
              </td>
              <td class={[table.td, 'text-right']}>
                <Button
                  size="sm"
                  variant="secondary-destructive"
                  onclick={() => revoke(invitation)}
                >
                  Revocar
                </Button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </LayerCard>
{/if}
