-- SundayStage migration 0003 — converge on the `church` tenant (platform Fase 0)
--
-- Funn A from the Sunday-platform analysis: SundayStage's cloud backend used its
-- OWN tenant root (`library` + `library_member` + `is_member(lib)`), divergent
-- from SundayPlan's `church` + `church_member` + `is_member_of(church_id)`. With
-- two incompatible tenant models the suite cannot share one identity. This
-- migration makes `church` the tenant root and demotes `library` to a
-- sub-entity of a church — so the schema matches SundayPlan exactly and the two
-- projects can later merge behind one Sunday account.
--
-- Strategy (least-invasive): introduce church/church_member with the SAME shape
-- and helper names as SundayPlan, give `library` a `church_id`, backfill one
-- church per existing library, then REDEFINE the existing `is_member(lib)` /
-- `can_write(lib)` helpers to resolve membership through the library's church.
-- Every content policy in 0002_rls.sql keeps working unchanged — it just becomes
-- church-scoped. Role vocabulary maps owner→admin, editor→planner,
-- operator→viewer.
--
-- Idempotent and safe on an empty DB (test phase, no users yet). Not yet
-- applied/verified here — needs Supabase (Docker unavailable). Apply with
-- `supabase db push`.

-- ── Church (tenant root) — mirrors SundayPlan 0001_tenancy ───────────────────
create table if not exists public.church (
  id            uuid primary key default gen_random_uuid(),
  name          text not null,
  slug          text not null unique check (slug ~ '^[a-z0-9-]+$'),
  plan_tier     text not null default 'free' check (plan_tier in ('free','starter','growth','network')),
  locale        text not null default 'no',
  timezone      text not null default 'Europe/Oslo',
  denomination  text,
  created_at    timestamptz not null default now(),
  updated_at    timestamptz not null default now()
);
create index if not exists church_slug_idx on public.church (slug);
alter table public.church enable row level security;

create table if not exists public.church_member (
  church_id   uuid not null references public.church(id) on delete cascade,
  user_id     uuid not null references auth.users(id)    on delete cascade,
  role        text not null check (role in ('admin','planner','team_lead','viewer')),
  created_at  timestamptz not null default now(),
  primary key (church_id, user_id)
);
create index if not exists church_member_user_idx on public.church_member (user_id);
alter table public.church_member enable row level security;

-- ── Membership helpers — same names/signatures as SundayPlan ──────────────────
create or replace function public.is_member_of(check_church_id uuid)
returns boolean language sql security definer stable set search_path = public as $$
  select exists (
    select 1 from public.church_member
    where church_id = check_church_id and user_id = auth.uid()
  );
$$;

create or replace function public.is_planner_of(check_church_id uuid)
returns boolean language sql security definer stable set search_path = public as $$
  select exists (
    select 1 from public.church_member
    where church_id = check_church_id and user_id = auth.uid()
      and role in ('admin','planner','team_lead')
  );
$$;

-- Church-level policies (mirror SundayPlan).
create policy church_member_read on public.church
  for select using (public.is_member_of(id));
create policy church_planner_update on public.church
  for update using (public.is_planner_of(id));
create policy church_member_read_self on public.church_member
  for select using (user_id = auth.uid() or public.is_planner_of(church_id));

-- updated_at trigger (reuse the existing touch_updated_at from 0001_schema).
drop trigger if exists trg_touch_church on public.church;
create trigger trg_touch_church before update on public.church
  for each row execute function public.touch_updated_at();

-- ── library becomes a sub-entity of church ───────────────────────────────────
alter table public.library
  add column if not exists church_id uuid references public.church(id) on delete cascade;

-- Backfill: one church per existing library (church id = library id for a clean
-- 1:1), then link. Slug 'lib-<hex>' satisfies the ^[a-z0-9-]+$ check and is
-- unique per library.
insert into public.church (id, name, slug)
  select id, name, 'lib-' || replace(id::text, '-', '')
    from public.library
  on conflict (id) do nothing;

update public.library set church_id = id where church_id is null;

-- Backfill membership from the library owner + existing library_member rows,
-- mapping the role vocabulary.
insert into public.church_member (church_id, user_id, role)
  select l.church_id, l.owner_id, 'admin'
    from public.library l
  on conflict do nothing;

insert into public.church_member (church_id, user_id, role)
  select l.church_id, lm.user_id,
         case lm.role
           when 'owner'  then 'admin'
           when 'editor' then 'planner'
           else 'viewer'
         end
    from public.library_member lm
    join public.library l on l.id = lm.library_id
  on conflict do nothing;

-- Every library now belongs to a church.
alter table public.library alter column church_id set not null;
create index if not exists idx_library_church on public.library (church_id);

-- ── Redefine the gatekeepers to resolve through the library's church ──────────
-- Same signatures as 0002_rls.sql, so all content policies (song/section/
-- arrangement/service/service_item) that call is_member(library_id) /
-- can_write(library_id) keep working — now church-scoped. library_member is
-- retained for backward-compat but is no longer the authority; church_member is.
create or replace function public.is_member(lib uuid)
returns boolean language sql security definer stable set search_path = public as $$
  select exists (
    select 1
      from public.library l
      join public.church_member cm on cm.church_id = l.church_id
     where l.id = lib and cm.user_id = auth.uid()
  );
$$;

create or replace function public.can_write(lib uuid)
returns boolean language sql security definer stable set search_path = public as $$
  select exists (
    select 1
      from public.library l
      join public.church_member cm on cm.church_id = l.church_id
     where l.id = lib and cm.user_id = auth.uid()
       and cm.role in ('admin','planner','team_lead')
  );
$$;
