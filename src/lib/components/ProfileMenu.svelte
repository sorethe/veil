<script lang="ts">
  import type { Profile } from "../api";

  let {
    profiles,
    activeId,
    onselect,
    onmanage,
    disabled = false,
    reason,
  }: {
    profiles: Profile[];
    activeId: string | null;
    onselect: (id: string) => void;
    onmanage: () => void;
    disabled?: boolean;
    reason?: string;
  } = $props();
</script>

<div class="profile-menu">
  <label class="kicker" for="active-profile">Profile</label>
  <div class="profile-menu__row">
    <select
      id="active-profile"
      class="select"
      {disabled}
      title={disabled ? reason : undefined}
      onchange={(event) => onselect(event.currentTarget.value)}
    >
      <option value="" disabled selected={activeId === null}>Select a profile</option>
      {#each profiles as profile (String(profile.id))}
        <option
          value={String(profile.id)}
          selected={activeId !== null && String(profile.id) === String(activeId)}
        >
          {profile.name} — {profile.kind}
        </option>
      {/each}
    </select>
    <button type="button" class="btn" onclick={onmanage}>Manage</button>
  </div>
  {#if disabled && reason}
    <p class="profile-menu__reason">{reason}</p>
  {/if}
</div>

<style>
  .profile-menu {
    display: grid;
    gap: 5px;
    min-width: 0;
    width: min(320px, 100%);
  }

  .profile-menu__row {
    display: flex;
    gap: 6px;
    min-width: 0;
  }

  .profile-menu__row .select {
    flex: 1;
    min-width: 0;
  }

  .profile-menu__reason {
    font-size: var(--fs-2xs);
    color: var(--faint);
  }
</style>
