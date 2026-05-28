-- Phase 9 — Row Level Security.
--
-- Every syncable row is scoped to a library; a user only sees rows in libraries
-- they're a member of. Reads require membership; writes require an editor/owner
-- role (operators are read-only). Mirrors the plan's owner/editor/operator
-- model. Not yet applied/verified here — needs Supabase.

-- ── Membership helpers (SECURITY DEFINER to avoid recursive RLS) ────────────
create or replace function public.is_member(lib uuid)
returns boolean language sql security definer stable as $$
  select exists (
    select 1 from public.library_member
    where library_id = lib and user_id = auth.uid()
  );
$$;

create or replace function public.can_write(lib uuid)
returns boolean language sql security definer stable as $$
  select exists (
    select 1 from public.library_member
    where library_id = lib and user_id = auth.uid()
      and role in ('owner', 'editor')
  );
$$;

-- ── Enable RLS everywhere ────────────────────────────────────────────────────
alter table public.library         enable row level security;
alter table public.library_member  enable row level security;
alter table public.song            enable row level security;
alter table public.song_section    enable row level security;
alter table public.arrangement     enable row level security;
alter table public.service         enable row level security;
alter table public.service_item    enable row level security;

-- ── library ──────────────────────────────────────────────────────────────────
create policy library_read on public.library
  for select using (public.is_member(id));
create policy library_owner_write on public.library
  for all using (owner_id = auth.uid()) with check (owner_id = auth.uid());

-- ── library_member: a user sees membership rows of libraries they belong to;
--    only owners manage membership. ─────────────────────────────────────────
create policy member_read on public.library_member
  for select using (public.is_member(library_id));
create policy member_owner_manage on public.library_member
  for all using (
    exists (select 1 from public.library l
            where l.id = library_id and l.owner_id = auth.uid())
  ) with check (
    exists (select 1 from public.library l
            where l.id = library_id and l.owner_id = auth.uid())
  );

-- ── Library-scoped content: read = member, write = editor/owner ──────────────
do $$
declare t text;
begin
  foreach t in array array[
    'song','song_section','arrangement','service','service_item'
  ] loop
    execute format(
      'create policy %1$s_read on public.%1$s
         for select using (public.is_member(library_id));', t);
    execute format(
      'create policy %1$s_write on public.%1$s
         for all using (public.can_write(library_id))
         with check (public.can_write(library_id));', t);
  end loop;
end $$;
