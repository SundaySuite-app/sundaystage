-- Phase 9 — SundayStage cloud schema (Supabase / Postgres).
--
-- The cloud mirror of the local SQLite syncable entities. Local SQLite stays
-- the running app's source of truth; this is best-effort background
-- replication (Sunday Pro). Every row carries sync metadata
-- (updated_at, deleted_at) and is scoped to a library; access is governed by
-- library_member (see 0002_rls.sql).
--
-- NOTE: not yet applied/verified here (needs Docker + Supabase). Apply with
-- `supabase db push`. See backend/README.md.

create extension if not exists "pgcrypto";

-- A library is the top-level sync scope. Shared via library_member.
create table if not exists public.library (
    id          uuid primary key default gen_random_uuid(),
    name        text not null,
    owner_id    uuid not null references auth.users (id) on delete cascade,
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz
);

-- Team membership + roles (Phase 9.2). owner > editor > operator.
create table if not exists public.library_member (
    library_id  uuid not null references public.library (id) on delete cascade,
    user_id     uuid not null references auth.users (id) on delete cascade,
    role        text not null default 'editor'
                check (role in ('owner', 'editor', 'operator')),
    created_at  timestamptz not null default now(),
    primary key (library_id, user_id)
);

create table if not exists public.song (
    id                uuid primary key default gen_random_uuid(),
    library_id        uuid not null references public.library (id) on delete cascade,
    title             text not null,
    author            text,
    ccli_song_id      text,
    tono_work_id      text,
    copyright_notice  text,
    default_key       text,
    tempo_bpm         int,
    language          text not null default 'no',
    updated_at        timestamptz not null default now(),
    deleted_at        timestamptz
);
create index if not exists idx_song_library on public.song (library_id);

create table if not exists public.song_section (
    id          uuid primary key default gen_random_uuid(),
    song_id     uuid not null references public.song (id) on delete cascade,
    library_id  uuid not null references public.library (id) on delete cascade,
    label       text not null,
    lyrics      text not null default '',
    position    int not null default 0,
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz
);
create index if not exists idx_section_song on public.song_section (song_id);

create table if not exists public.arrangement (
    id          uuid primary key default gen_random_uuid(),
    song_id     uuid not null references public.song (id) on delete cascade,
    library_id  uuid not null references public.library (id) on delete cascade,
    name        text not null,
    section_ids jsonb not null default '[]',
    is_default  boolean not null default false,
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz
);

create table if not exists public.service (
    id          uuid primary key default gen_random_uuid(),
    library_id  uuid not null references public.library (id) on delete cascade,
    name        text not null,
    starts_at   timestamptz not null,
    notes       text,
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz
);
create index if not exists idx_service_library on public.service (library_id);

create table if not exists public.service_item (
    id           uuid primary key default gen_random_uuid(),
    service_id   uuid not null references public.service (id) on delete cascade,
    library_id   uuid not null references public.library (id) on delete cascade,
    position     int not null default 0,
    kind         text not null,
    song_id      uuid references public.song (id) on delete set null,
    payload      jsonb not null default '{}',
    updated_at   timestamptz not null default now(),
    deleted_at   timestamptz
);

-- Keep updated_at fresh on every write.
create or replace function public.touch_updated_at()
returns trigger language plpgsql as $$
begin
  new.updated_at = now();
  return new;
end;
$$;

do $$
declare t text;
begin
  foreach t in array array[
    'library','song','song_section','arrangement','service','service_item'
  ] loop
    execute format(
      'drop trigger if exists trg_touch_%1$s on public.%1$s;
       create trigger trg_touch_%1$s before update on public.%1$s
       for each row execute function public.touch_updated_at();', t);
  end loop;
end $$;
