<script lang="ts">
  import { DropdownMenu } from 'bits-ui';

  import Menu from '#lib/components/Menu.svelte';
  import MenuItem from '#lib/components/MenuItem.svelte';
  import MenuRadioItem from '#lib/components/MenuRadioItem.svelte';
  import { menu } from '#lib/ui.js';

  /** A menu with a radio group, a link and an action, as in the sidebar. */
  let { onaction }: { onaction: () => void } = $props();
  let fruit = $state('apple');
</script>

<Menu label="Frutas">
  {#snippet trigger()}Abrir{/snippet}
  <DropdownMenu.RadioGroup bind:value={fruit}>
    <DropdownMenu.GroupHeading class={menu.heading}>Fruta</DropdownMenu.GroupHeading>
    <MenuRadioItem value="apple">Manzana</MenuRadioItem>
    <MenuRadioItem value="banana">Banana</MenuRadioItem>
    <MenuRadioItem value="cherry" closeOnSelect={false}>Cereza</MenuRadioItem>
  </DropdownMenu.RadioGroup>
  <DropdownMenu.Separator class={menu.separator} />
  <MenuItem href="#perfil">Perfil</MenuItem>
  <MenuItem onSelect={onaction}>Salir</MenuItem>
</Menu>
<output>{fruit}</output>
